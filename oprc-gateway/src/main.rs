use std::error::Error;

use envconfig::Envconfig;
use oprc_gateway::{start_server, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_log();
    let config = Config::init_from_env()?;
    start_server(config).await?;
    Ok(())
}

fn init_log() {
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::{
        layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("FLARE_LOG")
                .from_env_lossy(),
        )
        .init();
}
