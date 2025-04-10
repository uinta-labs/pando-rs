use anyhow::Result;
use async_nats::Subject;
use bollard::container::{KillContainerOptions, StartContainerOptions};
use bollard::secret::{PortBinding, PortMap, SystemVersionPlatform};
use bollard::{container::ListContainersOptions, Docker, API_DEFAULT_VERSION};
use bytes::Bytes;
use futures::StreamExt;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use tokio::{task, time};
use tracing::debug;
use uuid::Uuid;

use crate::config_json::ConfigJson;
use crate::grpc_remote::{
    CheckAnonymousDeviceRegistrationRequest, ContainerPortDefinition, RegistrationFailureStatus,
    Schedule,
};
use crate::temp::{list_zones, Temperature};
use crate::{config, registration};

const DEFAULT_DOCKER_ENGINE_SOCKET: &str = "/run/balena-engine.sock";

#[derive(Debug)]
struct Runner {
    docker: Docker,
    host_socket_path: String,
}

impl Runner {
    async fn new(socket: &str) -> Result<Self, bollard::errors::Error> {
        let docker = Docker::connect_with_unix(socket, 120, API_DEFAULT_VERSION)?;
        let version = docker.version().await?;
        println!(
            "Connected to docker engine {} {} {} {}",
            version.os.unwrap_or("Unknown".to_string()),
            version.arch.unwrap_or("Unknown".to_string()),
            version.api_version.unwrap_or("Unknown".to_string()),
            version
                .platform
                .unwrap_or(SystemVersionPlatform {
                    name: "Unknown".to_string(),
                })
                .name
        );
        Ok(Runner {
            docker,
            host_socket_path: socket.to_string(),
        })
    }

    async fn list_containers_matching_label(
        &self,
        label: &str,
        value: &str,
    ) -> Result<Vec<bollard::models::ContainerSummary>, bollard::errors::Error> {
        let mut filters = HashMap::new();
        let filter_value = format!("{}={}", label, value);
        filters.insert("label", vec![filter_value.as_str()]);

        let options = ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        };

        self.docker.list_containers(Some(options)).await
    }

    async fn kill_container(&self, container_id: &str) -> Result<(), bollard::errors::Error> {
        self.docker
            .kill_container(container_id, None::<KillContainerOptions<String>>)
            .await
    }

    async fn image_exists_locally(&self, image: &str) -> Result<bool, bollard::errors::Error> {
        self.docker
            .image_history(image)
            .await
            .map(|_| true)
            .or_else(|_| Ok(false))
    }

    async fn pull_image(&self, image: &str) -> Result<(), bollard::errors::Error> {
        let options = bollard::image::CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);
        while let Some(result) = stream.next().await {
            match result {
                Ok(_) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    async fn run_container(
        &self,
        image: &str,
        command: Option<Vec<String>>,
        env: Vec<String>,
        labels: HashMap<String, String>,
        bind_docker_socket: bool,
        network_mode_host: bool,
        ports: Vec<ContainerPortDefinition>,
    ) -> Result<String, bollard::errors::Error> {
        let mut binds = Vec::new();
        if bind_docker_socket {
            binds.push(format!("{}:/var/run/docker.sock", self.host_socket_path));
        }

        // Convert String vectors to string slice vectors
        let cmd: Option<Vec<&str>> = command
            .as_ref()
            .map(|c| c.iter().map(|s| s.as_str()).collect());
        let env_refs: Vec<&str> = env.iter().map(|s| s.as_str()).collect();
        let labels_refs: HashMap<&str, &str> = labels
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let config = bollard::container::Config {
            image: Some(image),
            cmd,
            env: Some(env_refs),
            labels: Some(labels_refs),
            host_config: Some(bollard::models::HostConfig {
                port_bindings: Some(
                    ports
                        .iter()
                        .map(|port| {
                            let key = format!("{}/{}", port.container_port, port.protocol);
                            let value = vec![PortBinding {
                                host_ip: None,
                                host_port: Some(format!("{}", port.host_port)),
                            }];
                            (key, Some(value))
                        })
                        .collect::<PortMap>(),
                ),
                binds: Some(binds),
                network_mode: if network_mode_host {
                    Some("host".to_string())
                } else {
                    None
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let container = self
            .docker
            .create_container(
                None::<bollard::container::CreateContainerOptions<String>>,
                config,
            )
            .await?;

        self.docker
            .start_container(&container.id, None::<StartContainerOptions<String>>)
            .await?;

        Ok(container.id)
    }
}

async fn apply_schedule(
    runner: &Runner,
    schedule: &Schedule,
) -> Result<(), Box<dyn std::error::Error>> {
    // List existing containers
    let existing_containers = runner
        .list_containers_matching_label("io.uinta.pando.managed", "true")
        .await?;

    // Track currently running containers
    let mut currently_running = HashMap::new();
    for container in existing_containers {
        let container_id = container.id.unwrap_or_default();
        let labels = container.labels.unwrap_or_default();

        let task_id = labels.get("io.uinta.pando.task-id");
        if let Some(task_id) = task_id {
            let mut found = false;
            for task in &schedule.containers {
                if task.id == *task_id {
                    found = true;
                    currently_running.insert(task_id.clone(), true);
                    break;
                }
            }

            if !found {
                println!("Removing container {}", container_id);
                if let Err(e) = runner.kill_container(&container_id).await {
                    println!("Error removing container: {:?}", e);
                }
            }
        }
    }

    if schedule.id.is_empty() {
        println!("No schedule to run");
        return Ok(());
    }

    println!("Running schedule: {}", schedule.id);

    // Start new containers
    for task in &schedule.containers {
        if currently_running.contains_key(&task.id) {
            println!("Task {} already running", task.id);
            continue;
        }

        println!("Running task: {}", task.name);

        if !runner.image_exists_locally(&task.container_image).await? {
            if let Err(e) = runner.pull_image(&task.container_image).await {
                println!("Error pulling image: {:?}", e);
                continue;
            }
        }

        let env_vars: Vec<String> = task
            .environment
            .iter()
            .map(|e| format!("{}={}", e.key, e.value))
            .collect();

        let command = if !task.command.is_empty() {
            Some(task.command.clone())
        } else {
            None
        };

        let mut labels = HashMap::new();
        labels.insert("io.uinta.pando.task-id".to_string(), task.id.clone());
        labels.insert("io.uinta.pando.task-name".to_string(), task.name.clone());
        labels.insert(
            "io.uinta.pando.schedule-id".to_string(),
            schedule.id.clone(),
        );

        match runner
            .run_container(
                &task.container_image,
                command,
                env_vars,
                labels,
                task.bind_docker_socket,
                task.network_mode == "host",
                task.ports.clone(),
            )
            .await
        {
            Ok(container_id) => println!("Container {}({}) started", task.id, container_id),
            Err(e) => println!("Error running container: {:?}", e),
        }
    }

    Ok(())
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SystemStats {
    cpu_temp: f64,
}

#[derive(Debug)]
enum MessageSubject {
    SetSchedule,
    GetSchedule,
    GetStats,
}

fn parse_subject(s: Subject) -> Result<MessageSubject, anyhow::Error> {
    let parts = s.split(".").collect::<Vec<&str>>();

    if parts.len() < 3 {
        return Err(anyhow::anyhow!("Invalid subject (too short)"));
    }

    if parts[0] != "pando" {
        return Err(anyhow::anyhow!(
            "Unrecognized subject (first segment was not 'pando')"
        ));
    }

    if parts[1] != "commands" {
        return Err(anyhow::anyhow!(
            "Unrecognized subject (second segment was not 'commands')"
        ));
    }

    match parts[2] {
        "run-schedule" => Ok(MessageSubject::SetSchedule),
        "get-schedule" => Ok(MessageSubject::GetSchedule),
        "get-stats" => Ok(MessageSubject::GetStats),
        _ => Err(anyhow::anyhow!("Invalid subject")),
    }
}

fn parse_schedule_payload(payload: Bytes) -> Result<Schedule, anyhow::Error> {
    // prost::Message::decode(payload).map_err(|e| anyhow::anyhow!(e))
    Schedule::decode(payload).map_err(|e| anyhow::anyhow!(e))
    // serde_json::from_slice(&payload).map_err(|e| anyhow::anyhow!(e))
}

async fn run_scheduler(runner: Runner, device_id: String) -> Result<(), anyhow::Error> {
    // let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "tls://connect.ngs.global".to_string());
    let nats_url = env::var("NATS_URL").unwrap_or_else(|_| "mqtt.stag9.com".to_string());
    // let client = async_nats::ConnectOptions::with_credentials_file(
    //     "/Users/isaac/Downloads/NGS-Default-Example-Client.creds",
    // )
    debug!("Connecting to NATS server at {}", nats_url);
    let client = async_nats::ConnectOptions::new()
        // .await
        // .expect("Failed to create client")
        .name(format!("pando-agent-{}", device_id))
        .connect(nats_url)
        .await?;
    let mut subscriber = client.subscribe("pando.commands.*").await?;

    task::spawn(async move {
        loop {
            match list_zones().await {
                Ok(temp_zones) => {
                    for zone in temp_zones {
                        let temp = Temperature::new(zone.clone());
                        let temp = temp.get_temperature().await.unwrap();
                        let stats = SystemStats { cpu_temp: temp };
                        let stats_json = serde_json::to_string(&stats).unwrap();
                        println!("Publishing stats: {}", stats_json);

                        let subject = format!("pando.stats.{}.json", device_id);
                        if let Err(e) = client.publish(subject, stats_json.into()).await {
                            println!("Error publishing stats: {:?}", e);
                        }
                        time::sleep(Duration::from_secs(5)).await;
                    }
                }
                Err(e) => println!("Error getting temperature: {:?}", e),
            }
        }
    });

    loop {
        while let Some(message) = subscriber.next().await {
            println!("Received message {:?}", message);

            match parse_subject(message.subject.clone()) {
                Err(e) => {
                    println!("Error parsing subject: {:?}", e);
                    continue;
                }
                Ok(subject) => match subject {
                    MessageSubject::SetSchedule => match parse_schedule_payload(message.payload) {
                        Err(e) => {
                            println!("Error parsing schedule payload: {:?}", e);
                            continue;
                        }
                        Ok(schedule) => {
                            if let Err(e) = apply_schedule(&runner, &schedule).await {
                                println!("Error applying schedule: {:?}", e);
                            }

                            println!("Received schedule: {:?}", schedule);
                        }
                    },
                    MessageSubject::GetSchedule => {
                        println!("Received get-schedule request (unhandled)");
                    }
                    MessageSubject::GetStats => {
                        println!("Received get-stats request (unhandled)");
                    }
                },
            }
        }
        println!("Subscriber closed. Attempting to reconnect...");
        time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn run_agent() -> Result<(), anyhow::Error> {
    let docker_engine_socket =
        env::var("DOCKER_HOST").unwrap_or_else(|_| DEFAULT_DOCKER_ENGINE_SOCKET.to_string());

    let runner = Runner::new(&docker_engine_socket).await?;

    let uname = rustix::system::uname();
    let hostname = uname.nodename().to_str().unwrap_or("unknown");
    let device_id = hostname.to_string();

    let mut config_manager = config::Config::<ConfigJson>::new(
        #[cfg(target_os = "linux")]
        config::ConfigMode::Path("/boot/config.json".into()),
        #[cfg(not(target_os = "linux"))]
        config::ConfigMode::User,
        "config.json".into(),
    )?;
    let config_json = config_manager.setup()?;

    if registration::get_registration_status(&config_json) {
        debug!("Device is already registered: {:?}", config_json);
    } else {
        println!("Device is not registered");
        let api_endpoint = config_json
            .api_endpoint
            .clone()
            .unwrap_or(
                env::var("PANDO_API_ENDPOINT").map_err(|_| {
                    anyhow::anyhow!("No API endpoint specified in config and failed to load using PANDO_API_ENDPOINT env var")
                })?,
            );

        let temporary_device_identifier = Uuid::now_v7();

        let mut grpc_client =
            registration::wait_for_client_connection(api_endpoint.clone()).await?;
        let anon_registration_response = registration::start_anonymous_provisioning(
            temporary_device_identifier,
            &mut grpc_client,
        )
        .await?;

        println!("Please use the following registration token or URL to claim your device:");
        println!("{}", anon_registration_response.registration_token);
        println!("{}", anon_registration_response.registration_url);

        let (api_token, device_identifier) = loop {
            let resp = grpc_client
                .check_anonymous_device_registration(CheckAnonymousDeviceRegistrationRequest {
                    temporary_device_identifier: temporary_device_identifier.to_string(),
                })
                .await;

            match resp {
                Ok(response) => {
                    match response.into_inner().result {
                        Some(
                            crate::grpc_remote::check_anonymous_device_registration_response::Result::RegistrationResult(
                                result,
                            ),
                        ) => {
                            println!("Device registered successfully");
                            println!("API Token: {}", result.api_token);
                            println!("Device ID: {}", result.device_identifier);

                            break (
                                result.api_token,
                                result.device_identifier,
                            );
                        }
                        Some(
                            crate::grpc_remote::check_anonymous_device_registration_response::Result::RegistrationPending(
                                result,
                            ),
                        ) => {
                            println!("Device registration is still pending");
                            println!("Time remaining: {} {}", result.seconds_until_timeout, match result.seconds_until_timeout {
                                0 => "seconds".to_string(),
                                1 => "second".to_string(),
                                _ => "seconds".to_string(),
                            });
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                        Some(registration_result) => {
                            match registration_result {
                                crate::grpc_remote::check_anonymous_device_registration_response::Result::RegistrationFailureStatus(v) => {
                                    if v == RegistrationFailureStatus::Unspecified as i32 {
                                        println!("Device registration failed: unspecified error");
                                    } else if v == RegistrationFailureStatus::TokenExpired as i32 {
                                        println!("Device registration failed: token expired");
                                    } else if v == RegistrationFailureStatus::TokenInvalid as i32 {
                                        println!("Device registration failed: token invalid");
                                    } else {
                                        println!("Device registration failed: unknown error");
                                    }
                                },
                                _ => {
                                    println!("Device registration failed: unspecified error");
                                }
                            }
                            break (String::new(), String::new());
                        }
                        None => {
                            println!("No result received from server");
                        }
                    }
                }
                Err(e) => {
                    println!("Error checking registration status: {:?}", e);
                }
            }
        };

        config_manager.data_mut().api_token = Some(api_token);
        config_manager.data_mut().api_endpoint = Some(api_endpoint);
        config_manager.data_mut().uuid = Some(device_identifier);
        config_manager.save()?;
    }

    run_scheduler(runner, device_id).await
}
