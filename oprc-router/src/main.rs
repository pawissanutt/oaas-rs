use std::error::Error;

use envconfig::Envconfig;

pub fn init_log() {
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::{
        layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("OPRC_LOG")
                .from_env_lossy(),
        )
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    init_log();
    let mut z_conf = oprc_zenoh::OprcZenohConfig::init_from_env()?;
    z_conf.mode = zenoh_config::WhatAmI::Router;
    let conf = z_conf.create_zenoh();
    let _session = match zenoh::open(conf).await {
        Ok(runtime) => runtime,
        Err(e) => {
            println!("{e}. Exiting...");
            std::process::exit(-1);
        }
    };

    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("failed to install signal handler")
        .recv()
        .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    Ok(())
}
