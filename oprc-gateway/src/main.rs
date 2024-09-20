use std::error::Error;

use envconfig::Envconfig;
use oprc_gateway::{start_server, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    let config = Config::init_from_env()?;
    start_server(config).await?;
    Ok(())
}
