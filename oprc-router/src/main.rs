use std::error::Error;

use envconfig::Envconfig;

#[derive(Envconfig, Clone, Debug)]
pub struct RouterConfig {
    #[envconfig(from = "OPRC_ZENOH_PORT", default = "7447")]
    pub zenoh_port: u16,

    #[envconfig(from = "OPRC_ZENOH_PEERS")]
    pub peers: Option<String>,
}

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
    let z_conf = oprc_zenoh::OprcZenohConfig::init_from_env()?;
    let conf = z_conf.create_zenoh();
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
