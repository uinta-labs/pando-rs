use clap::{Parser, Subcommand};
use pando_core::schedule::Spec;

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

#[derive(Debug, Subcommand, Clone)]
enum AppSubCommand {
    Schedule(ScheduleCommand),
}

#[tokio::main]
async fn main() -> anyhow::Result<(), anyhow::Error> {
    env_logger::init();
    let args = Args::parse();

    match args.subcommand {
        AppSubCommand::Schedule(db_cmd) => match db_cmd.schedule_subcommand {
            ScheduleSubcommand::Emit { schedule_path, device_id, remote_service_endpoint } => {
                let spec = Spec::read_from(&schedule_path)?;
                println!("{:?}", spec);
                // open nats client, emit schedule
            }
        },
    }

    Ok(())
}