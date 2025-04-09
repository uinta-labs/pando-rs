use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    tokio::select! {
        _ = async {
            pando_core::daemon::run_agent().await.map_err(|e| {
                eprintln!("Agent error: {}", e);
                std::process::exit(1);
            })
        } => println!("Agent exited"),
        _ = sigterm.recv() => println!("Received SIGTERM"),
        _ = sigint.recv() => println!("Received SIGINT"),
    }

    println!("Shutting down");
    Ok(())
}
