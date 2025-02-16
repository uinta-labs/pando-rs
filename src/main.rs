mod config;

// this is still a mess
// mod config_txt;

use bollard::container::{KillContainerOptions, StartContainerOptions};
use bollard::{container::ListContainersOptions, Docker, API_DEFAULT_VERSION};
use futures::StreamExt;
use std::collections::HashMap;
use std::env;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time;
use tonic::Request;

use crate::grpc_remote::container::NetworkMode;
use grpc_remote::remote_service_client::RemoteServiceClient;
use grpc_remote::{GetScheduleRequest, Schedule};

pub mod grpc_remote {
    tonic::include_proto!("remote.upd88.com");
}

const DEFAULT_BASE_URL: &str = "https://graphene.fluffy-broadnose.ts.net";
const DEFAULT_DOCKER_ENGINE_SOCKET: &str = "/run/balena-engine.sock";

#[derive(Debug)]
struct Runner {
    docker: Docker,
    host_socket_path: String,
}

impl Runner {
    fn new(socket: &str) -> Result<Self, bollard::errors::Error> {
        let docker = Docker::connect_with_unix(socket, 120, API_DEFAULT_VERSION)?;
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

    println!("Running schedule: {}", schedule.id);

    // Start new containers
    for task in &schedule.containers {
        if currently_running.contains_key(&task.id) {
            println!("Task {} already running", task.id);
            continue;
        }

        println!("Running task: {}", task.name);
        if let Err(e) = runner.pull_image(&task.container_image).await {
            println!("Error pulling image: {:?}", e);
            continue;
        }

        let mut env_vars = Vec::new();
        for (key, value) in &task.env {
            env_vars.push(format!("{}={}", key, value));
        }

        let command = if !task.command.is_empty() {
            Some(vec![task.command.clone()])
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
                task.network_mode == <NetworkMode as Into<i32>>::into(NetworkMode::Host), // Assuming 1 is HOST in the proto enum
            )
            .await
        {
            Ok(container_id) => println!("Container {}({}) started", task.id, container_id),
            Err(e) => println!("Error running container: {:?}", e),
        }
    }

    Ok(())
}

async fn run_scheduler_tick(
    client: &mut RemoteServiceClient<tonic::transport::Channel>,
    runner: &Runner,
    device_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Running scheduler");

    let request = Request::new(GetScheduleRequest {
        device_id: device_id.to_string(),
    });

    match client.get_schedule(request).await {
        Ok(response) => {
            if let Some(schedule) = response.into_inner().schedule {
                if let Err(e) = apply_schedule(runner, &schedule).await {
                    println!("Error applying schedule: {:?}", e);
                }
            } else {
                println!("Received empty schedule");
            }
        }
        Err(e) => println!("Error getting schedule: {:?}", e),
    }

    Ok(())
}

async fn wait_for_client_connection(
    endpoint: String,
) -> Result<RemoteServiceClient<tonic::transport::Channel>, Box<dyn std::error::Error>> {
    let total_wait_limit = 600; // we will wait for up to 10 minutes before bailing
    let connect_start_time = time::Instant::now();
    let current_wait_period_sec = 5;
    loop {
        if connect_start_time.elapsed().as_secs() > total_wait_limit {
            return Err(format!("Timed out waiting {} seconds for remote service to connect", total_wait_limit).into());
        }
        let client = RemoteServiceClient::connect(endpoint.clone()).await;
        match client {
            Ok(client) => {
                println!("Connected to remote service");
                return Ok(client);
            }
            Err(e) => {
                println!("Error connecting to remote service (have been trying for {}/{} seconds; retrying in {} second(s)): {:?}", connect_start_time.elapsed().as_secs(), total_wait_limit, current_wait_period_sec, e);
                time::sleep(Duration::from_secs(current_wait_period_sec)).await;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let docker_engine_socket =
        env::var("DOCKER_HOST").unwrap_or_else(|_| DEFAULT_DOCKER_ENGINE_SOCKET.to_string());
    println!("Connecting to docker engine at {}", docker_engine_socket);

    let runner = Runner::new(&docker_engine_socket)?;

    let endpoint = env::var("API_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    println!("API_URL: {}", endpoint);

    let uname = rustix::system::uname();
    let hostname = uname.nodename().to_str().unwrap_or("unknown");
    let device_id = hostname.to_string();

    // Set up signal handling
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = async {
            match wait_for_client_connection(endpoint).await {
                Ok(mut client) => {
                    loop {
                        if let Err(e) = run_scheduler_tick(&mut client, &runner, &device_id).await {
                            println!("Scheduler tick error: {:?}", e);
                        }
                        time::sleep(Duration::from_secs(15)).await;
                    }
                }
                Err(e) => {
                    println!("Giving up on connecting to remote service {:?}", e);
                    return;
                }
            }
        } => {}
        _ = sigterm.recv() => println!("Received SIGTERM"),
        _ = sigint.recv() => println!("Received SIGINT"),
    }

    println!("Shutting down");
    Ok(())
}
