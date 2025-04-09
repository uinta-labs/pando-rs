use prost::Message;

use crate::grpc_remote::Schedule;

// TODO: Reconsider this wrapper

#[derive(Debug, Clone)]
pub struct Client {
    endpoint: String,
}

impl Client {
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    pub async fn shutdown(&self) {}

    pub async fn emit_schedule(
        &self,
        _device_id: String,
        schedule: &Schedule,
    ) -> Result<(), anyhow::Error> {
        // let hostname = rustix::system::uname().nodename().to_string_lossy().to_string();

        // let client = async_nats::ConnectOptions::with_credentials_file(
        //     "/Users/isaac/Downloads/NGS-Default-Agent-Publisher.creds",
        // )
        let client = async_nats::ConnectOptions::new()
            // .await
            // .expect("Failed to create client")
            .name("pando-cli-abc123".to_string())
            .connect(self.endpoint.clone())
            .await?;

        println!("Connection state: {:?}", client.connection_state());

        // let subject = format!("pando.schedule.{}", device_id);
        let subject = "pando.commands.run-schedule".to_string();
        // let mut buf = vec![];
        // let containers: Vec<Container> = spec
        //     .services
        //     .iter()
        //     .map(|service| {
        //         let container = Container {
        //             id: uuid::Uuid::now_v7().to_string(),
        //             name: service.name.clone(),
        //             container_image: service.image.clone(),
        //             command: service.command.clone(),
        //             environment: service
        //                 .environment
        //                 .iter()
        //                 .map(|(k, v)| ContainerEnvironment {
        //                     key: k.clone(),
        //                     value: v.clone(),
        //                 })
        //                 .collect(),
        //             privileged: service.privileged,
        //             network_mode: "".to_string(), // not yet supported on spec side
        //             ports: vec![],
        //             bind_docker_socket: service.host_features.daemon_socket,
        //             bind_boot: service.host_features.boot_partition,
        //             entrypoint: "".to_string(), // not yet supported on spec side
        //         };
        //         // container.encode(&mut buf).unwrap();
        //         container
        //     })
        //     .collect();

        // let proto_schedule = Schedule {
        //     id: uuid::Uuid::now_v7().to_string(),
        //     current: true,
        //     containers,
        // };

        // serde_json::to_writer(&mut buf, &proto_schedule)?;
        // serde_json::to_writer(&mut buf, &spec)?;
        // proto_schedule.encode(&mut buf).unwrap();

        // let buf = serde_json::to_vec(schedule)?;
        let mut buf = vec![];
        schedule.encode(&mut buf).unwrap();

        println!("Publishing schedule to {}", subject);
        match client.publish(subject, buf.into()).await {
            Ok(_) => {
                client.flush().await?;
                println!("Published schedule");
            }
            Err(e) => eprintln!("Failed to publish schedule: {:?}", e),
        }

        Ok(())
    }
}
