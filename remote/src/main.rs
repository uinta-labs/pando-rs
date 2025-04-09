use clap::Parser;
use dotenvy::{EnvLoader, EnvSequence};
use migration::{Migrator, MigratorTrait};
use pando_core::grpc_remote::{
    check_anonymous_device_registration_response, RegistrationFailureStatus,
};
use sea_orm::ActiveValue::Set;
use sea_orm::ColumnTrait;
use sea_orm::{ActiveModelTrait, ConnectOptions, QueryFilter};
use sea_orm::{Database, DatabaseConnection, EntityTrait};
use std::time::Duration;
use std::{fs::File, io::ErrorKind, path::Path};
use tokio::time::sleep;
use tonic::transport::Server;
use tonic::{Response, Status};
use tracing::{debug, info, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

use entity::device::Entity as Device;
use entity::schedule::ActiveModel as ScheduleModel;
use entity::schedule::Entity as ScheduleEntity;

use entity::{device, waiting_room};

#[derive(Debug)]
pub(crate) struct PandoRemoteServer {
    nats_client: pando_core::nats::Client,
    connection: DatabaseConnection,
    user_base_url: String,
}

#[tonic::async_trait]
impl pando_core::grpc_remote::remote_service_server::RemoteService for PandoRemoteServer {
    async fn get_available_devices(
        &self,
        request: tonic::Request<pando_core::grpc_remote::GetAvailableDevicesRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::GetAvailableDevicesResponse>, tonic::Status>
    {
        debug!("Received GetAvailableDevicesRequest {:?}", request);

        let all_devices = Device::find().all(&self.connection).await.map_err(|e| {
            tracing::error!("Failed to fetch devices: {}", e);
            tonic::Status::internal("Failed to fetch devices")
        })?;
        debug!("Fetched {} devices", all_devices.len());

        Ok(tonic::Response::new(
            pando_core::grpc_remote::GetAvailableDevicesResponse {
                devices: all_devices
                    .into_iter()
                    .map(
                        |device| pando_core::grpc_remote::get_available_devices_response::Device {
                            id: device.id.to_string(),
                            name: format!("Device {}", device.id),
                        },
                    )
                    .collect(),
            },
        ))
    }

    async fn publish_schedule(
        &self,
        request: tonic::Request<pando_core::grpc_remote::PublishScheduleRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::PublishScheduleResponse>, tonic::Status>
    {
        let schedule_body = request.into_inner().schedule.ok_or_else(|| {
            tracing::error!("Schedule is required");
            tonic::Status::invalid_argument("Schedule is required")
        })?;

        debug!("Received PublishScheduleRequest {:?}", schedule_body);

        let schedule_body_json = serde_json::to_string(&schedule_body).map_err(|e| {
            tracing::error!("Failed to serialize schedule body: {}", e);
            tonic::Status::internal("Failed to serialize schedule body")
        })?;

        let schedule_record = ScheduleEntity::insert(ScheduleModel {
            id: Set(Uuid::now_v7()),
            body: Set(schedule_body_json.into()),
        })
        .exec_with_returning(&self.connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to insert schedule record: {}", e);
            tonic::Status::internal("Failed to insert schedule record")
        })?;

        let all_devices = Device::find().all(&self.connection).await.map_err(|e| {
            tracing::error!("Failed to fetch devices: {}", e);
            tonic::Status::internal("Failed to fetch devices")
        })?;

        for device in all_devices {
            debug!("Publishing schedule to device {:?}", device.id);

            self.nats_client
                .emit_schedule(device.id.to_string(), &schedule_body)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to publish schedule: {}", e);
                    tonic::Status::internal("Failed to publish schedule")
                })?;
        }

        Ok(tonic::Response::new(
            pando_core::grpc_remote::PublishScheduleResponse {
                schedule_id: schedule_record.id.to_string(),
            },
        ))
    }

    async fn set_device_schedule(
        &self,
        _request: tonic::Request<pando_core::grpc_remote::SetDeviceScheduleRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::SetDeviceScheduleResponse>, tonic::Status>
    {
        todo!()
    }

    async fn claim_device(
        &self,
        request: tonic::Request<pando_core::grpc_remote::ClaimDeviceRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::ClaimDeviceResponse>, tonic::Status> {
        let registration_token = request.into_inner().registration_token.trim().to_string();
        debug!("Received ClaimDeviceRequest {:?}", registration_token);

        sleep(std::time::Duration::from_secs(1)).await;

        if registration_token.is_empty() {
            return Err(tonic::Status::invalid_argument(
                "Registration token is required",
            ));
        }

        // setup device record
        let device_record = device::ActiveModel {
            id: Set(Uuid::now_v7()),
            // FIXME FIXME FIXME FIXME FIXME FIXME
            fleet_id: todo!(),
        };

        let new_device = Device::insert(device_record)
            .exec_with_returning(&self.connection)
            .await
            .map_err(|e| {
                tracing::error!("Failed to insert device record: {}", e);
                tonic::Status::internal("Failed to insert device record")
            })?;
        debug!("Device record created: {:?}", new_device);

        // update waiting room record
        let waiting_room_record = waiting_room::Entity::find()
            .filter(waiting_room::Column::RegistrationToken.eq(&registration_token))
            .one(&self.connection)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch waiting room record: {}", e);
                tonic::Status::internal("Failed to fetch waiting room record")
            })?
            .ok_or_else(|| {
                tracing::error!(
                    "No waiting room record found for token: {}",
                    registration_token
                );
                tonic::Status::not_found("No waiting room record found")
            })?;

        debug!("Waiting room record found: {:?}", waiting_room_record);
        let updated_waiting_room_record = waiting_room::ActiveModel {
            resulting_device_id: Set(Some(new_device.id)),
            api_endpoint: Set(Some(format!("{}/api/v1", self.user_base_url))),
            api_token: Set(Some(Uuid::new_v4().to_string())),
            ..waiting_room_record.into()
        };

        updated_waiting_room_record
            .update(&self.connection)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update waiting room record: {}", e);
                tonic::Status::internal("Failed to update waiting room record")
            })?;

        Ok(tonic::Response::new(
            pando_core::grpc_remote::ClaimDeviceResponse {
                device_identifier: new_device.id.to_string(),
            },
        ))
    }
}

impl PandoRemoteServer {
    async fn get_or_create_waiting_room_record(
        &self,
        temporary_device_identifier: String,
    ) -> Result<waiting_room::Model, Status> {
        match waiting_room::Entity::find()
            .filter(waiting_room::Column::DeviceTemporaryToken.eq(&temporary_device_identifier))
            .one(&self.connection)
            .await
        {
            Ok(Some(waiting_room_record)) => {
                debug!("Found waiting room record: {:?}", waiting_room_record);
                Ok(waiting_room_record)
            }
            Ok(None) => {
                debug!("No waiting room record found, creating a new one");
                let registration_token = Uuid::new_v4().to_string().replace("-", "").to_uppercase();
                let registration_url = format!("{}/a/{}", &self.user_base_url, registration_token);

                let waiting_room_insert_record = waiting_room::ActiveModel {
                    id: Set(Uuid::now_v7()),
                    first_seen: Set(chrono::Utc::now().naive_utc()),
                    expires_at: Set(chrono::Utc::now()
                        .naive_utc()
                        .checked_add_signed(chrono::Duration::seconds(60))
                        .unwrap()),
                    device_temporary_token: Set(temporary_device_identifier),
                    registration_token: Set(registration_token),
                    registration_url: Set(registration_url),
                    resulting_device_id: Set(None),
                    api_endpoint: Set(None),
                    api_token: Set(None),
                };

                let waiting_room_record = waiting_room_insert_record
                    .insert(&self.connection)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to insert waiting room record: {}", e);
                        tonic::Status::internal("Failed to insert waiting room record")
                    })?;
                Ok(waiting_room_record)
            }
            Err(e) => {
                tracing::error!("Failed to fetch waiting room record: {}", e);
                Err(tonic::Status::internal(
                    "Failed to fetch waiting room record",
                ))
            }
        }
    }
}

#[tonic::async_trait]
impl pando_core::grpc_remote::device_service_server::DeviceService for PandoRemoteServer {
    //   rpc StartAnonymousDeviceRegistration(StartAnonymousDeviceRegistrationRequest) returns (StartAnonymousDeviceRegistrationResponse);

    async fn start_anonymous_device_registration(
        &self,
        request: tonic::Request<pando_core::grpc_remote::StartAnonymousDeviceRegistrationRequest>,
    ) -> Result<
        tonic::Response<pando_core::grpc_remote::StartAnonymousDeviceRegistrationResponse>,
        tonic::Status,
    > {
        let temporary_device_identifier = request.into_inner().temporary_device_identifier;
        debug!(
            "Received StartAnonymousDeviceRegistrationRequest {:?}",
            temporary_device_identifier
        );

        let waiting_room_record = self
            .get_or_create_waiting_room_record(temporary_device_identifier.clone())
            .await?;

        debug!("Waiting room record created: {:?}", waiting_room_record);

        let response = pando_core::grpc_remote::StartAnonymousDeviceRegistrationResponse {
            registration_token: waiting_room_record.registration_token,
            registration_url: waiting_room_record.registration_url,
        };

        Ok(Response::new(response))
    }

    // async fn wait_for_anonymous_device_registration(
    //     &self,
    //     request: tonic::Request<
    //         Streaming<pando_core::grpc_remote::WaitForAnonymousDeviceRegistrationRequest>,
    //     >,
    // ) -> Result<
    //     tonic::Response<pando_core::grpc_remote::WaitForAnonymousDeviceRegistrationResponse>,
    //     tonic::Status,
    // > {
    //     debug!("Received WaitForAnonymousDeviceRegistrationRequest");

    //     let mut stream = request.into_inner();

    //     let (temporary_device_identifier_channel, mut temporary_device_identifier_receiver) =
    //         mpsc::channel(1);

    //     tokio::spawn(async move {
    //         while let Some(request) = stream.next().await {
    //             match request {
    //                 Ok(request) => {
    //                     let temporary_device_identifier = request.temporary_device_identifier;
    //                     debug!(
    //                         "Received temporary device identifier: {:?}",
    //                         temporary_device_identifier
    //                     );
    //                     if temporary_device_identifier.is_empty() {
    //                         return Err(tonic::Status::invalid_argument(
    //                             "Temporary device identifier is required",
    //                         ));
    //                     }
    //                     if let Err(_) = temporary_device_identifier_channel
    //                         .send(temporary_device_identifier)
    //                         .await
    //                     {
    //                         tracing::error!("Failed to send temporary device identifier");
    //                     }
    //                 }
    //                 Err(e) => {
    //                     tracing::error!("Failed to read request: {}", e);
    //                     return Err(tonic::Status::internal("Failed to read request"));
    //                 }
    //             }
    //         }

    //         Ok(())
    //     });

    //     // wait for the temporary device identifier to be sent
    //     let temporary_device_identifier = temporary_device_identifier_receiver
    //         .recv()
    //         .await
    //         .ok_or_else(|| {
    //             tracing::error!("Failed to receive temporary device identifier");
    //             tonic::Status::cancelled("Failed to receive temporary device identifier")
    //         })?;
    //     debug!(
    //         "Received temporary device identifier: {:?}",
    //         temporary_device_identifier
    //     );

    //     if temporary_device_identifier.is_empty() {
    //         return Err(tonic::Status::invalid_argument(
    //             "Temporary device identifier is required",
    //         ));
    //     }

    //     let mut waiting_room_stream = WaitingRoom::find()
    //         .filter(waiting_room::Column::DeviceTemporaryToken.eq(temporary_device_identifier))
    //         .stream(&self.connection)
    //         .await
    //         .map_err(|e| {
    //             tracing::error!("Failed to fetch waiting room record: {}", e);
    //             tonic::Status::internal("Failed to fetch waiting room record")
    //         })?;

    //     loop {
    //         while let Some(waiting_room_record_result) = waiting_room_stream.next().await {
    //             match waiting_room_record_result {
    //                 Ok(waiting_room_record_result) => {
    //                     if waiting_room_record_result.api_endpoint.is_some()
    //                         && waiting_room_record_result.api_token.is_some()
    //                         && waiting_room_record_result.resulting_device_id.is_some()
    //                     {
    //                         info!("Device registered: {:?}", waiting_room_record_result);
    //                         let resulting_device_id = waiting_room_record_result
    //                             .resulting_device_id
    //                             .unwrap()
    //                             .to_string();
    //                         let api_endpoint = waiting_room_record_result.api_endpoint.unwrap();
    //                         let api_token = waiting_room_record_result.api_token.unwrap();

    //                         return Ok(Response::new(
    //                         pando_core::grpc_remote::WaitForAnonymousDeviceRegistrationResponse {
    //                             device_identifier: resulting_device_id,
    //                             api_endpoint,
    //                             api_token,
    //                         },
    //                     ));
    //                     }
    //                 }
    //                 Err(e) => {
    //                     tracing::error!("Failed to fetch waiting room record: {}", e);
    //                     return Err(tonic::Status::internal(
    //                         "Failed to fetch waiting room record",
    //                     ));
    //                 }
    //             }
    //         }
    //     }

    //     // loop {
    //     //     // every second, check to see if the device is registered
    //     //     // if it is, return the device identifier and api endpoint

    //     //     let temporary_device_identifier = temporary_device_identifier_receiver.recv().await;

    //     //     if temporary_device_identifier.is_none() {
    //     //         return Err(tonic::Status::cancelled(
    //     //             "WaitForAnonymousDeviceRegistration cancelled",
    //     //         ));
    //     //     }
    //     //     let temporary_device_identifier = temporary_device_identifier.unwrap();
    //     //     debug!(
    //     //         "Received temporary device identifier: {:?}",
    //     //         temporary_device_identifier
    //     //     );
    //     //     if temporary_device_identifier.is_empty() {
    //     //         return Err(tonic::Status::invalid_argument(
    //     //             "Temporary device identifier is required",
    //     //         ));
    //     //     }
    //     //     debug!(
    //     //         "Known temporary device identifier: {:?}",
    //     //         temporary_device_identifier
    //     //     );
    //     //     // lookup the waiting room record
    //     //     let waiting_room_record = self
    //     //         .get_or_create_waiting_room_record(temporary_device_identifier.clone())
    //     //         .await?;
    //     //     info!("Waiting room record found: {:?}", waiting_room_record);

    //     //     if waiting_room_record.api_endpoint.is_some()
    //     //         && waiting_room_record.api_token.is_some()
    //     //         && waiting_room_record.resulting_device_id.is_some()
    //     //     {
    //     //         let resulting_device_id = waiting_room_record.resulting_device_id.unwrap();
    //     //         let api_endpoint = waiting_room_record.api_endpoint.unwrap();
    //     //         let api_token = waiting_room_record.api_token.unwrap();

    //     //         // TODO: mark waiting room record as used
    //     //         // maybe delete or soft delete?

    //     //         // clean up channel

    //     //         println!("Device registered: {:?}", resulting_device_id);

    //     //         return Ok(Response::new(
    //     //             pando_core::grpc_remote::WaitForAnonymousDeviceRegistrationResponse {
    //     //                 device_identifier: resulting_device_id.to_string(),
    //     //                 api_endpoint,
    //     //                 api_token,
    //     //             },
    //     //         ));
    //     //     }

    //     //     let now_utc = chrono::Utc::now().naive_utc().and_utc().timestamp() as u64;
    //     //     let expires_at_utc = waiting_room_record.expires_at.and_utc().timestamp() as u64;
    //     //     let remaining_seconds: Option<u64> = if expires_at_utc > now_utc {
    //     //         Some(expires_at_utc - now_utc)
    //     //     } else {
    //     //         None
    //     //     };
    //     //     debug!("Remaining seconds until timeout: {:?}", remaining_seconds);

    //     //     sleep(std::time::Duration::from_secs(1)).await;
    //     // }

    //     // let mut known_temporary_device_identifier = String::new();

    //     // while let Some(request) = stream.next().await {
    //     //     match request {
    //     //         Ok(request) => {
    //     //             let temporary_device_identifier = request.temporary_device_identifier;
    //     //             debug!(
    //     //                 "Received temporary device identifier: {:?}",
    //     //                 temporary_device_identifier
    //     //             );
    //     //             if temporary_device_identifier.is_empty() {
    //     //                 return Err(tonic::Status::invalid_argument(
    //     //                     "Temporary device identifier is required",
    //     //                 ));
    //     //             }
    //     //             if known_temporary_device_identifier.is_empty() {
    //     //                 known_temporary_device_identifier = temporary_device_identifier.clone();
    //     //             } else if known_temporary_device_identifier != temporary_device_identifier {
    //     //                 return Err(tonic::Status::invalid_argument(
    //     //                     "Temporary device identifier must not change while waiting",
    //     //                 ));
    //     //             }
    //     //             debug!(
    //     //                 "Known temporary device identifier: {:?}",
    //     //                 known_temporary_device_identifier
    //     //             );

    //     //             // lookup the waiting room record
    //     //             let waiting_room_record = self
    //     //                 .get_or_create_waiting_room_record(temporary_device_identifier.clone())
    //     //                 .await?;
    //     //             debug!("Waiting room record found: {:?}", waiting_room_record);

    //     //             if waiting_room_record.api_endpoint.is_some()
    //     //                 && waiting_room_record.api_token.is_some()
    //     //                 && waiting_room_record.resulting_device_id.is_some()
    //     //             {
    //     //                 let resulting_device_id = waiting_room_record.resulting_device_id.unwrap();
    //     //                 let api_endpoint = waiting_room_record.api_endpoint.unwrap();
    //     //                 let api_token = waiting_room_record.api_token.unwrap();

    //     //                 // TODO: mark waiting room record as used
    //     //                 // maybe delete or soft delete?

    //     //                 return Ok(Response::new(
    //     //                     pando_core::grpc_remote::WaitForAnonymousDeviceRegistrationResponse {
    //     //                         device_identifier: resulting_device_id.to_string(),
    //     //                         api_endpoint,
    //     //                         api_token,
    //     //                     },
    //     //                 ));
    //     //             }

    //     //             let now_utc = chrono::Utc::now().naive_utc().and_utc().timestamp() as u64;
    //     //             let expires_at_utc =
    //     //                 waiting_room_record.expires_at.and_utc().timestamp() as u64;
    //     //             let remaining_seconds: Option<u64> = if expires_at_utc > now_utc {
    //     //                 Some(expires_at_utc - now_utc)
    //     //             } else {
    //     //                 None
    //     //             };
    //     //             debug!("Remaining seconds until timeout: {:?}", remaining_seconds);
    //     //         }
    //     //         Err(e) => {
    //     //             tracing::error!("Failed to read request: {}", e);
    //     //             return Err(tonic::Status::internal("Failed to read request"));
    //     //         }
    //     //     }
    //     // }

    //     Err(tonic::Status::cancelled(
    //         "WaitForAnonymousDeviceRegistration cancelled",
    //     ))
    // }

    // type WaitForAnonymousDeviceRegistrationStream = Pin<
    //     Box<dyn Stream<Item = Result<WaitForAnonymousDeviceRegistrationResponse, Status>> + Send>,
    // >;

    // async fn wait_for_anonymous_device_registration(
    //     &self,
    //     request: tonic::Request<pando_core::grpc_remote::WaitForAnonymousDeviceRegistrationRequest>,
    //     // ) -> WaitForAnonymousDeviceRegistrationResult<ResponseStream> {
    // ) -> Result<tonic::Response<Self::WaitForAnonymousDeviceRegistrationStream>, tonic::Status>
    // {
    //     let temporary_device_identifier = request
    //         .into_inner()
    //         .temporary_device_identifier
    //         .trim()
    //         .to_string();
    //     if temporary_device_identifier.is_empty() {
    //         return Err(tonic::Status::invalid_argument(
    //             "Temporary device identifier is required",
    //         ));
    //     }
    //     debug!(
    //         "Received AnonymousDeviceRegistrationRequest {:?}",
    //         temporary_device_identifier
    //     );

    //     let db_connection_mutex: Arc<tokio::sync::Mutex<DatabaseConnection>> =
    //         Arc::new(tokio::sync::Mutex::new(self.connection.clone()));

    //     let (tx, rx) = tokio::sync::mpsc::channel(4);

    //     match &self
    //         .get_or_create_waiting_room_record(temporary_device_identifier.clone())
    //         .await
    //     {
    //         Ok(waiting_room_record) => {
    //             debug!("Waiting room record created: {:?}", waiting_room_record);
    //         }
    //         Err(e) => {
    //             tracing::error!("Failed to create waiting room record: {}", e);
    //             return Err(tonic::Status::internal(
    //                 "Failed to create waiting room record",
    //             ));
    //         }
    //     }

    //     tokio::spawn(async move {
    //         loop {
    //             match waiting_room::Entity::find()
    //                 .filter(
    //                     waiting_room::Column::DeviceTemporaryToken
    //                         .eq(temporary_device_identifier.clone()),
    //                 )
    //                 .one(&*db_connection_mutex.lock().await)
    //                 .await
    //             {
    //                 Ok(Some(waiting_room_record)) => {
    //                     debug!("Found waiting room record: {:?}", waiting_room_record);
    //                     let now_utc = chrono::Utc::now().naive_utc().and_utc().timestamp() as u64;
    //                     let expires_at_utc =
    //                         waiting_room_record.expires_at.and_utc().timestamp() as u64;
    //                     let remaining_seconds: Option<u64> = if expires_at_utc > now_utc {
    //                         Some(expires_at_utc - now_utc)
    //                     } else {
    //                         None
    //                     };
    //                     debug!("Remaining seconds until timeout: {:?}", remaining_seconds);

    //                     // let remaining_seconds = waiting_room_record.expires_at.and_utc().timestamp()
    //                     //     as u64
    //                     //     - chrono::Utc::now().timestamp() as u64;
    //                     if remaining_seconds.is_some() {
    //                         let response = pando_core::grpc_remote::DeviceRegistrationPending {
    //                             seconds_until_timeout: remaining_seconds.unwrap(),
    //                             registration_token: waiting_room_record.registration_token.clone(),
    //                             registration_url: waiting_room_record.registration_url.clone(),
    //                         };
    //                         // tx.send(Ok(response)).await.unwrap();
    //                         match tx.send(Ok(WaitForAnonymousDeviceRegistrationResponse {
    //                             result: Some(
    //                                 pando_core::grpc_remote::wait_for_anonymous_device_registration_response::Result::RegistrationPending(
    //                                     response.clone(),
    //                                 )
    //                             )
    //                         })).await {
    //                             Ok(_)  => {
    //                                 debug!("Sent response: {:?}", response.clone());
    //                             }
    //                             Err(e) => {
    //                                 tracing::error!("Failed to send response: {}", e);
    //                                 break;
    //                             }
    //                         }
    //                     } else {
    //                         // Token expired, throw an error
    //                         tx.send(Err(tonic::Status::failed_precondition(
    //                             "Token expired".to_string(),
    //                         )))
    //                         .await
    //                         .unwrap();
    //                         break;
    //                     }
    //                     sleep(std::time::Duration::from_secs(1)).await;
    //                 }
    //                 Ok(None) => {
    //                     // No waiting room record found, send an error
    //                     tx.send(Err(tonic::Status::not_found(
    //                         "No waiting room record found".to_string(),
    //                     )))
    //                     .await
    //                     .unwrap();
    //                     break;
    //                 }
    //                 Err(e) => {
    //                     tracing::error!("Failed to fetch waiting room record: {}", e);
    //                     tx.send(Err(tonic::Status::internal(
    //                         "Failed to fetch waiting room record".to_string(),
    //                     )))
    //                     .await
    //                     .unwrap();
    //                     break;
    //                 }
    //             }
    //         }
    //     });
    //     // for feature in &features[..] {
    //     //     if in_range(feature.location.as_ref().unwrap(), request.get_ref()) {
    //     //         tx.send(Ok(feature.clone())).await.unwrap();
    //     //     }
    //     // }

    //     let output_stream = ReceiverStream::new(rx);
    //     Ok(Response::new(
    //         Box::pin(output_stream) as Self::WaitForAnonymousDeviceRegistrationStream
    //     ))
    // }

    async fn check_anonymous_device_registration(
        &self,
        request: tonic::Request<pando_core::grpc_remote::CheckAnonymousDeviceRegistrationRequest>,
    ) -> Result<
        tonic::Response<pando_core::grpc_remote::CheckAnonymousDeviceRegistrationResponse>,
        tonic::Status,
    > {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let temporary_device_identifier = request
            .into_inner()
            .temporary_device_identifier
            .trim()
            .to_string();
        if temporary_device_identifier.is_empty() {
            return Ok(
                tonic::Response::new(
                    pando_core::grpc_remote::CheckAnonymousDeviceRegistrationResponse {
                        result: Some(
                            check_anonymous_device_registration_response::Result::RegistrationFailureStatus(
                                RegistrationFailureStatus::TokenInvalid.into()
                            ),
                        )
                    },
                ),
            );
        }

        match &self
            .get_or_create_waiting_room_record(temporary_device_identifier.clone())
            .await
        {
            Ok(waiting_room_record) => {
                debug!("Waiting room record created: {:?}", waiting_room_record);

                debug!("Found waiting room record: {:?}", waiting_room_record);
                let now_utc = chrono::Utc::now().naive_utc().and_utc().timestamp() as u64;
                let expires_at_utc = waiting_room_record.expires_at.and_utc().timestamp() as u64;
                let remaining_seconds: Option<u64> = if expires_at_utc > now_utc {
                    Some(expires_at_utc - now_utc)
                } else {
                    None
                };

                debug!("Remaining seconds until timeout: {:?}", remaining_seconds);
                if remaining_seconds.is_some() {
                    if waiting_room_record.api_endpoint.is_some()
                        && waiting_room_record.api_token.is_some()
                        && waiting_room_record.resulting_device_id.is_some()
                    {
                        info!("Device registered: {:?}", waiting_room_record);
                        let resulting_device_id =
                            waiting_room_record.resulting_device_id.unwrap().to_string();
                        let api_endpoint = waiting_room_record.api_endpoint.clone().unwrap();
                        let api_token = waiting_room_record.api_token.clone().unwrap();

                        return Ok(Response::new(

                            pando_core::grpc_remote::CheckAnonymousDeviceRegistrationResponse {
                                result: Some(
                                    pando_core::grpc_remote::check_anonymous_device_registration_response::Result::RegistrationResult(
                                        pando_core::grpc_remote::RegitrationResult {
                                            device_identifier: resulting_device_id,
                                            api_endpoint,
                                            api_token,
                                        },
                                    ),
                                ),
                            },
                        ));
                    }

                    let response = pando_core::grpc_remote::DeviceRegistrationPending {
                        seconds_until_timeout: remaining_seconds.unwrap() as i64,
                        registration_token: waiting_room_record.registration_token.clone(),
                        registration_url: waiting_room_record.registration_url.clone(),
                    };
                    return Ok(
                        tonic::Response::new(
                            pando_core::grpc_remote::CheckAnonymousDeviceRegistrationResponse {
                                result: Some(
                                    pando_core::grpc_remote::check_anonymous_device_registration_response::Result::RegistrationPending(
                                        response.clone(),
                                    ),
                                ),
                            },
                        ),
                    );
                } else {
                    return Ok(
                        tonic::Response::new(
                            pando_core::grpc_remote::CheckAnonymousDeviceRegistrationResponse {
                                result: Some(
                                    pando_core::grpc_remote::check_anonymous_device_registration_response::Result::RegistrationFailureStatus(
                                        RegistrationFailureStatus::TokenExpired.into(),
                                    ),
                                ),
                            },
                        ),
                    );
                }
            }
            Err(e) => {
                tracing::error!("Failed to create waiting room record: {}", e);
                return Err(tonic::Status::internal(
                    "Failed to create waiting room record",
                ));
            }
        }
    }

    async fn device_registration(
        &self,
        _request: tonic::Request<pando_core::grpc_remote::DeviceRegistrationRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::DeviceRegistrationResponse>, tonic::Status>
    {
        todo!()
    }

    async fn get_schedule(
        &self,
        _request: tonic::Request<pando_core::grpc_remote::GetScheduleRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::GetScheduleResponse>, tonic::Status> {
        todo!()
    }

    async fn report_schedule_state(
        &self,
        _request: tonic::Request<pando_core::grpc_remote::ReportScheduleStateRequest>,
    ) -> Result<tonic::Response<pando_core::grpc_remote::ReportScheduleStateResponse>, tonic::Status>
    {
        todo!()
    }
}

fn load_env_if_present() -> anyhow::Result<()> {
    let path = Path::new(".env");
    match File::open(path) {
        Ok(file) => {
            EnvLoader::with_reader(file)
                .path(path)
                .sequence(EnvSequence::InputThenEnv)
                .load()?;
            Ok(())
        }
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                EnvLoader::default().sequence(EnvSequence::EnvOnly).load()?;
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[clap(long, short, default_value = "false")]
    verbose: bool,

    #[clap(long)]
    host: Option<String>,

    #[clap(long, short)]
    port: Option<u16>,

    #[clap(long, short)]
    database_url: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<(), anyhow::Error> {
    load_env_if_present().map_err(|e| anyhow::anyhow!("Failed to load env file: {}", e))?;
    env_logger::init();
    let args = Args::parse();

    let subscriber = FmtSubscriber::builder()
        .with_max_level(match args.verbose {
            true => Level::TRACE,
            false => Level::INFO,
        })
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    debug!("Verbose mode is enabled");

    let database_url = args.database_url.unwrap_or_else(|| {
        dotenvy::var("DATABASE_URL")
            .unwrap_or("postgres://pando_service:hunter2@localhost:54432/pandodb".to_string())
    });

    // USER_SITE_BASE_URL
    let user_base_url =
        dotenvy::var("USER_SITE_BASE_URL").unwrap_or("https://pando.upd88.com".to_string());

    let mut connection_options = ConnectOptions::new(&database_url);
    connection_options
        .max_connections(100)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(8))
        .sqlx_logging(true);
    // .sqlx_logging_level(log::LevelFilter::Info);

    let connection = Database::connect(connection_options)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;
    Migrator::up(&connection, None).await?;
    debug!("Database connection established");

    let nats_client = pando_core::nats::Client::new(
        dotenvy::var("NATS_URL").unwrap_or("nats://localhost:4222".to_string()),
    );

    let addr = format!(
        "{}:{}",
        args.host.unwrap_or("127.0.0.1".to_string()),
        args.port.unwrap_or(8900)
    );
    info!("Starting server at {}", addr);

    let remote_grpc_server = PandoRemoteServer {
        nats_client: nats_client.clone(),
        connection: connection.clone(),
        user_base_url: user_base_url.clone(),
    };

    let device_grpc_server = PandoRemoteServer {
        nats_client,
        connection,
        user_base_url,
    };

    Server::builder()
        .add_service(
            pando_core::grpc_remote::remote_service_server::RemoteServiceServer::new(
                remote_grpc_server,
            ),
        )
        .add_service(
            pando_core::grpc_remote::device_service_server::DeviceServiceServer::new(
                device_grpc_server,
            ),
        )
        .serve(addr.parse().unwrap())
        .await?;

    // let router = axum::Router::new()
    //     .route(
    //         "/api/v1/agent",
    //         axum::routing::get(|| async { "Hello, World!" }),
    //     )
    //     .route(
    //         "/api/v1/agent/:id",
    //         axum::routing::get(
    //             |axum::extract::Path(id): axum::extract::Path<String>| async move {
    //                 format!("Hello, {}!", id)
    //             },
    //         ),
    //     )
    //     .into_make_service();

    // let addr = format!(
    //     "{}:{}",
    //     args.host.unwrap_or("127.0.0.1".to_string()),
    //     args.port.unwrap_or(8080)
    // );
    // info!("Starting server at {}", addr);

    // let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    // axum::serve(listener, router).await.unwrap();

    // let listener = tokio::net::TcpListener::bind(&addr).await?;
    // let server = axum::Server::from_tcp(listener)?
    //     .serve(router)
    //     .with_graceful_shutdown(async {
    //         tokio::signal::ctrl_c()
    //             .await
    //             .expect("Failed to install Ctrl+C signal handler");
    //     });
    // tokio::spawn(async {
    //     if let Err(e) = server.await {
    //         eprintln!("Server error: {}", e);
    //     }
    // });
    // info!("Server started successfully");
    // info!("Waiting for shutdown signal...");
    // tokio::signal::ctrl_c()
    //     .await
    //     .expect("Failed to install Ctrl+C signal handler");
    // info!("Received shutdown signal");

    Ok(())
}
