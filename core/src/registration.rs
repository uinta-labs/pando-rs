use std::time::Duration;

use anyhow::bail;
use tokio::time;
use uuid::Uuid;

use crate::{config_json::ConfigJson, grpc_remote::{remote_service_client::RemoteServiceClient, AnonymousDeviceRegistrationRequest}};


pub fn get_registration_status(config: &ConfigJson) -> anyhow::Result<bool> {
    if config.api_token.is_none() {
        return Ok(false);
    }
    if config.api_endpoint.is_none() {
        return Ok(false);
    }
    if config.uuid.is_none() {
        return Ok(false);
    }
    Ok(true)
}

pub fn get_registration_token_and_endpoint(config: &ConfigJson) -> anyhow::Result<(String, String)> {
    match &config.init {
        Some(init) => {
            let provisioning_token = match &init.provisioning_token {
                Some(token) => token,
                None => {
                    bail!("No provisioning token in init section")
                }
            };
            let api_endpoint = match &config.api_endpoint {
                Some(endpoint) => endpoint,
                None => {
                    bail!("No api endpoint in config")
                }
            };
            Ok((provisioning_token.clone(), api_endpoint.clone()))
        }
        None => {
            bail!("No init section in config")
        }
    }
}

async fn wait_for_client_connection(
    endpoint: String,
) -> Result<RemoteServiceClient<tonic::transport::Channel>, anyhow::Error> {
    let total_wait_limit = 600; // we will wait for up to 10 minutes before bailing
    let connect_start_time = time::Instant::now();
    let current_wait_period_sec = 5;
    loop {
        if connect_start_time.elapsed().as_secs() > total_wait_limit {
            bail!(
                "Timed out waiting {} seconds for remote service to connect",
                total_wait_limit
            )
        }
        let channel = tonic::transport::Channel::from_shared(endpoint.clone())?
            .connect_timeout(Duration::from_secs(current_wait_period_sec))
            .concurrency_limit(1)
            .keep_alive_timeout(Duration::from_secs(3600));

        let client = RemoteServiceClient::connect(channel).await;
        match client {
            Ok(client) => {
                return Ok(client);
            }
            Err(e) => {
                println!(
                    "Error connecting to remote service (have been trying for {}/{} seconds; retrying in {} second(s)): {:?}",
                    connect_start_time.elapsed().as_secs(),
                    total_wait_limit,
                    current_wait_period_sec,
                    e
                );
                time::sleep(Duration::from_secs(current_wait_period_sec)).await;
            }
        }
    }
}


pub async fn run_anonymous_provisioning(
    mut client: RemoteServiceClient<tonic::transport::Channel>,
) -> anyhow::Result<()> {
    let temporary_device_identifier = Uuid::now_v7().to_string();
    let registration_request = AnonymousDeviceRegistrationRequest {
        temporary_device_identifier,
    };

    let response = client
        .anonymous_device_registration(tonic::Request::new(registration_request))
        .await?;

    let response = response.into_inner();

    Ok(())
}