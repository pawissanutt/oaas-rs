use envconfig::Envconfig;
use oprc_gateway::{start_server, Config};

#[tokio::main]
async fn main() {
    init_log();
    if let Ok(conf) = Config::init_from_env() {
        if let Err(e) = start_server(conf).await {
            tracing::error!("Error: {:?}", e);
            std::process::exit(1);
        };
    } else {
        tracing::error!("Failed to load config from env");
        std::process::exit(1);
    };
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
                .with_env_var("OPRC_LOG")
                .from_env_lossy(),
        )
        .init();
}
