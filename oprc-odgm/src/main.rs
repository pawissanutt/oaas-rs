use std::error::Error;

use flare_dht::ServerArgs;
use tokio::signal;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    let flare_node = flare_dht::start_server(ServerArgs {
        leader: true,
        ..Default::default()
    })
    .await?;
    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
    info!("starting a clean up for shutdown");
    flare_node.leave().await;
    info!("done clean up");
    Ok(())
}
