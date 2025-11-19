use std::error::Error;

use envconfig::Envconfig;
use oprc_odgm::{OdgmConfig, create_collection};
use tokio::signal;
use tracing::{debug, info};

fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);
    init_log();
    info!(
        "Starting tokio runtime with {} worker threads",
        worker_threads
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { start().await.unwrap() });
}

async fn start() -> Result<(), Box<dyn Error>> {
    let conf = OdgmConfig::init_from_env()?;
    debug!("use odgm config: {:?}", conf);
    let odgm = oprc_odgm::start_server(&conf, None).await?.0;

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
        EnvFilter, layer::SubscriberExt, util::SubscriberInitExt,
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
