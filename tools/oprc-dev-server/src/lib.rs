pub mod config;
mod frontend;
pub mod stub_api;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use config::DevServerConfig;
use oprc_odgm::{ObjectDataGridManager, OdgmConfig};
use oprc_zenoh::pool::Pool;
use tower_http::cors::CorsLayer;
use tracing::info;

/// Start the dev server with the given configuration.
pub async fn start(config: DevServerConfig) -> anyhow::Result<()> {
    let port = config.port;

    // 1. Create Zenoh session in peer mode (loopback, no external peers)
    let z_config = oprc_zenoh::OprcZenohConfig::default();

    // 2. Start ODGM (creates Pool, MetaManager, ShardManager, watch stream)
    let odgm_config = OdgmConfig {
        http_port: port,
        node_id: Some(1),
        members: Some("1".into()),
        ..Default::default()
    };
    let (odgm, session_pool) =
        oprc_odgm::start_raw_server(&odgm_config, Some(z_config))
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    let odgm = Arc::new(odgm);

    // 3. Create collections from config
    for collection in &config.collections {
        let req = collection.to_create_request();
        info!(
            collection = %collection.name,
            functions = collection.functions.len(),
            "Creating collection"
        );
        odgm.metadata_manager.create_collection(req).await?;
    }

    // Wait for shards to be created (poll with timeout).
    // WASM compilation of large modules can take 10+ seconds.
    let expected_shards = config.collections.len() as u32;
    let mut attempts = 0;
    while attempts < 600 {
        let stats = odgm.shard_manager.get_stats().await;
        if stats.total_shards_created >= expected_shards {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }

    // Verify shards are created and ready
    for collection in &config.collections {
        let shards = odgm
            .shard_manager
            .get_shards_for_collection(&collection.name)
            .await;
        if let Some(shard) = shards.first() {
            // Wait for shard readiness (WASM module loading, etc.)
            let mut ready_attempts = 0;
            while ready_attempts < 100 {
                if *shard.watch_readiness().borrow() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                ready_attempts += 1;
            }
            info!(
                collection = %collection.name,
                shard_id = shard.meta().id,
                ready = *shard.watch_readiness().borrow(),
                "Shard ready"
            );
        } else {
            tracing::warn!(
                collection = %collection.name,
                "Shard not found after creation"
            );
        }
    }

    // 4. Build the HTTP router
    let router = build_dev_router(&config, &session_pool).await?;

    // 5. Serve
    let socket = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let listener = tokio::net::TcpListener::bind(socket).await?;
    print_banner(&config, port);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(odgm))
        .await?;

    Ok(())
}

async fn build_dev_router(
    config: &DevServerConfig,
    session_pool: &Pool,
) -> anyhow::Result<Router> {
    let z_session = session_pool
        .get_session()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Gateway REST/gRPC routes at /api/class/...
    let gateway = oprc_gateway::build_router(
        z_session.clone(),
        Duration::from_secs(30),
        false,
    );

    // Strip /api/gateway prefix for frontend compatibility:
    // Frontend calls /api/gateway/api/class/... → forward to /api/class/...
    let gateway_proxy = Router::new().nest("/api/gateway", gateway.clone());

    // Stub PM API for frontend (/api/v1/deployments, etc.)
    let stub = stub_api::build_stub_api(config);

    let router = gateway
        .merge(gateway_proxy)
        .merge(stub)
        .layer(CorsLayer::permissive());

    // Frontend fallback (embedded static files)
    #[cfg(feature = "frontend")]
    let router = router.fallback(axum::routing::get(frontend::serve_frontend));

    Ok(router)
}

fn print_banner(config: &DevServerConfig, port: u16) {
    info!("╔══════════════════════════════════════════╗");
    info!("║       OaaS Local Dev Server              ║");
    info!("╠══════════════════════════════════════════╣");
    info!("║  http://localhost:{:<24}║", port);
    info!("╚══════════════════════════════════════════╝");
    for c in &config.collections {
        let fn_names: Vec<&str> =
            c.functions.iter().map(|f| f.id.as_str()).collect();
        if fn_names.is_empty() {
            info!("  Collection: {} (no functions)", c.name);
        } else {
            info!("  Collection: {} → [{}]", c.name, fn_names.join(", "));
        }
    }
}

async fn shutdown_signal(odgm: Arc<ObjectDataGridManager>) {
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
    info!("Shutting down...");
    odgm.close().await;
}
