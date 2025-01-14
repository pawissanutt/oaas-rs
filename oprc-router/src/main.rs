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
    let mut conf = z_conf.create_zenoh();
    conf.transport.unicast.set_max_links(16).unwrap();
    let _session = match zenoh::open(conf).await {
        Ok(runtime) => runtime,
        Err(e) => {
            println!("{e}. Exiting...");
            std::process::exit(-1);
        }
    };

    futures_util::future::pending::<()>().await;

    Ok(())
}
