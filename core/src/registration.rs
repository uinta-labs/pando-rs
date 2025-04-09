use std::time::Duration;

use anyhow::bail;
use tokio::time;
use uuid::Uuid;

use crate::grpc_remote::{
    StartAnonymousDeviceRegistrationRequest, StartAnonymousDeviceRegistrationResponse,
};
use crate::{config_json::ConfigJson, grpc_remote::device_service_client::DeviceServiceClient};

pub fn get_registration_status(config: &ConfigJson) -> bool {
    if config.api_token.is_none() {
        return false;
    }
    if config.api_endpoint.is_none() {
        return false;
    }
    if config.uuid.is_none() {
        return false;
    }
    true
}

// pub fn get_registration_token_and_endpoint(
//     config: &ConfigJson,
// ) -> anyhow::Result<(String, String)> {
//     match &config.init {
//         Some(init) => {
//             let provisioning_token = match &init.provisioning_token {
//                 Some(token) => token,
//                 None => {
//                     bail!("No provisioning token in init section")
//                 }
//             };
//             let api_endpoint = match &config.api_endpoint {
//                 Some(endpoint) => endpoint,
//                 None => {
//                     bail!("No api endpoint in config")
//                 }
//             };
//             Ok((provisioning_token.clone(), api_endpoint.clone()))
//         }
//         None => {
//             bail!("No init section in config")
//         }
//     }
// }

pub(crate) async fn wait_for_client_connection(
    endpoint: String,
) -> Result<DeviceServiceClient<tonic::transport::Channel>, anyhow::Error> {
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

        let client = DeviceServiceClient::connect(channel).await;
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

/// This function starts the anonymous provisioning process using the 'waiting room' technique.
/// We generate a temporary device identifier and send it to the server.
/// The server responds with a token and URL. We can then present the token and/or URL to the user, possibly in the
/// form of a QR code. A user can claim the device by scanning the QR code or entering the token in a web interface
/// after authenticating with their account.
pub(crate) async fn start_anonymous_provisioning(
    temporary_device_identifier: Uuid,
    client: &mut DeviceServiceClient<tonic::transport::Channel>,
) -> anyhow::Result<StartAnonymousDeviceRegistrationResponse, anyhow::Error> {
    let registration_request = StartAnonymousDeviceRegistrationRequest {
        temporary_device_identifier: temporary_device_identifier.to_string(),
    };

    let response = client
        .start_anonymous_device_registration(tonic::Request::new(registration_request))
        .await?;
    let response = response.into_inner();
    println!("Received response: {:?}", response);
    Ok(response)

    // loop {
    //     match client
    //         .wait_for_anonymous_device_registration(tonic::Request::new(
    //             registration_request.clone(),
    //         ))
    //         .await
    //     {
    //         Ok(streaming_response) => {
    //             let mut stream = streaming_response.into_inner();
    //             while let Some(response) = stream.message().await? {
    //                 println!("Received response: {:?}", response);
    //             }
    //         }
    //         Err(e) => {
    //             // TODO: look to see if the error is because the temporary device identifier is already in use (somehow), and bail if so
    //             println!("Error during anonymous device registration: {:?}", e);
    //             time::sleep(Duration::from_secs(5)).await;
    //         }
    //     }
    // }
}
