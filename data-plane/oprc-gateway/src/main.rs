use envconfig::Envconfig;
use oprc_gateway::{Config, start_server};

fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);
    init_log();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { start().await });
}

async fn start() {
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
        EnvFilter, layer::SubscriberExt, util::SubscriberInitExt,
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
