use std::error::Error;

use envconfig::Envconfig;
use oprc_odgm::{create_collection, OdgmConfig};
use tokio::signal;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_log();
    let conf = OdgmConfig::init_from_env()?;
    let odgm = oprc_odgm::start_server(&conf).await?;

    create_collection(odgm.clone(), &conf).await;

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
    info!("starting a clean up for shutdown");
    odgm.close().await;
    info!("done clean up");
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
                .with_env_var("ODGM_LOG")
                .from_env_lossy(),
        )
        .init();
}
