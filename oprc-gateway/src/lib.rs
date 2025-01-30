mod conf;
mod error;
mod handler;
// mod id;
// mod rpc;
use std::time::Duration;

pub use conf::Config;
use envconfig::Envconfig;
use oprc_offload::{conn::PoolConfig, Invoker};
use tracing::info;

pub async fn start_server(
    config: Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let z_config = oprc_zenoh::OprcZenohConfig::init_from_env()?;
    let conf = PoolConfig {
        max_open: config.max_pool_size as u64,
        max_idle_lifetime: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    let offload_manager = Invoker::new(conf);
    offload_manager.start_sync(&config.pm_uri).await?;
    if config.print_pool_state_interval > 0 {
        offload_manager
            .print_pool_state_interval(config.print_pool_state_interval as u64);
    }

    let session = zenoh::open(z_config.create_zenoh()).await?;
    let router = handler::build_router(offload_manager, session);
    let listener =
        tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.http_port))
            .await?;
    info!("start server on port {:?}", config.http_port);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
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
}
