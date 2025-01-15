mod conf;
mod error;
mod handler;
// mod id;
mod route;
mod rpc;
use std::{sync::Arc, time::Duration};

pub use conf::Config;
use oprc_offload::conn::{ConnManager, PoolConfig};
use route::RoutingManager;
use tracing::info;

pub async fn start_server(
    config: Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let routing_manager = Arc::new(RoutingManager::new());
    let conf = PoolConfig {
        max_open: config.max_pool_size as u64,
        max_idle_lifetime: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    let conn_manager =
        Arc::new(ConnManager::new(routing_manager.clone(), conf));

    routing_manager.start_sync(&config.pm_uri).await?;
    if config.print_pool_state_interval > 0 {
        let sleep_time = config.print_pool_state_interval;
        let cm = conn_manager.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(sleep_time as u64))
                    .await;
                let states = cm.get_states().await;
                info!("pool state {:?}", states);
            }
        });
    }
    let router = handler::build_router(conn_manager);
    let listener =
        tokio::net::TcpListener::bind(format!("0.0.0.0:{}", config.http_port))
            .await?;
    info!("start server on port {:?}", config.http_port);
    axum::serve(listener, router).await?;
    Ok(())
}
