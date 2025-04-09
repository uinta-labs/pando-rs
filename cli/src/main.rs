use clap::{Parser, Subcommand};
use pando_core::{
    grpc_remote::{GetAvailableDevicesRequest, Schedule},
    schedule::Spec,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[clap(subcommand)]
    subcommand: AppSubCommand,
}

#[derive(Debug, Subcommand, Clone)]
enum ScheduleSubcommand {
    #[clap(name = "emit")]
    Emit {
        #[clap(long)]
        schedule_path: String,
        #[clap(long)]
        device_id: String,
        #[clap(long)]
        remote_service_endpoint: String,
    },
}

#[derive(Parser, Debug, Clone)]
struct ScheduleCommand {
    #[clap(subcommand)]
    schedule_subcommand: ScheduleSubcommand,
}

#[derive(Parser, Debug, Clone)]
struct DevicesCommand {
    #[clap(subcommand)]
    subcommand: DevicesSubCommand,
}

#[derive(Debug, Subcommand, Clone)]
enum DevicesSubCommand {
    #[clap(name = "list")]
    List {
        #[clap(long)]
        remote_service_endpoint: String,
    },

    #[clap(name = "claim")]
    Claim {
        #[clap(long)]
        remote_service_endpoint: String,
        #[clap(long)]
        registration_token: String,
    },
}

#[derive(Debug, Subcommand, Clone)]
enum AppSubCommand {
    Schedule(ScheduleCommand),
    Devices(DevicesCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<(), anyhow::Error> {
    env_logger::init();
    let args = Args::parse();

    match args.subcommand {
        AppSubCommand::Schedule(db_cmd) => {
            match db_cmd.schedule_subcommand {
                ScheduleSubcommand::Emit {
                    schedule_path,
                    device_id: _,
                    remote_service_endpoint,
                } => {
                    let spec = Spec::read_from(&schedule_path)?;
                    let mut schedule: pando_core::grpc_remote::Schedule =
                        Schedule::from_spec(&spec);
                    schedule.current = true;
                    schedule.id = uuid::Uuid::now_v7().to_string();

                    let mut grpc_client = pando_core::grpc_remote::remote_service_client::RemoteServiceClient::connect(remote_service_endpoint).await?;

                    grpc_client
                        .publish_schedule(pando_core::grpc_remote::PublishScheduleRequest {
                            schedule: Some(schedule),
                        })
                        .await?;
                }
            }
        }
        AppSubCommand::Devices(devices_cmd) => {
            match devices_cmd.subcommand {
                DevicesSubCommand::List {
                    remote_service_endpoint,
                } => {
                    let mut grpc_client = pando_core::grpc_remote::remote_service_client::RemoteServiceClient::connect(
                    remote_service_endpoint,
                ).await?;

                    let response = grpc_client
                        .get_available_devices(GetAvailableDevicesRequest {})
                        .await?;
                    let devices = response.into_inner().devices;
                    if devices.is_empty() {
                        println!("No devices available");
                    } else {
                        println!("Available devices:");
                    }
                    for device in devices {
                        println!("--------------------------------");
                        println!("\tDevice ID: {}", device.id);
                        println!("\tDevice Name: {}", device.name);
                        println!();
                    }
                }
                DevicesSubCommand::Claim {
                    remote_service_endpoint,
                    registration_token,
                } => {
                    let mut grpc_client = pando_core::grpc_remote::remote_service_client::RemoteServiceClient::connect(
                        remote_service_endpoint,
                    ).await?;

                    let response = grpc_client
                        .claim_device(pando_core::grpc_remote::ClaimDeviceRequest {
                            registration_token,
                        })
                        .await?;
                    let device_id = response.into_inner().device_identifier;
                    if device_id.is_empty() {
                        println!("Failed to claim device");
                    } else {
                        println!("Successfully claimed device with ID: {}", device_id);
                    }
                }
            }
        }
    }

    Ok(())
}
