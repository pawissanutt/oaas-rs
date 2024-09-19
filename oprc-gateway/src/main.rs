use std::error::Error;

use oprc_gateway::start_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    start_server().await?;
    Ok(())
}
