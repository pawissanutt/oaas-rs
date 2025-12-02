mod conf;
mod error;
mod handler;
pub mod metrics;

pub use conf::Config;
use envconfig::Envconfig;
pub use handler::build_router;
use oprc_observability::{TracingConfig, setup_tracing};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

pub async fn start_server(
    config: Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let json_format =
        match config.log_format.as_deref().map(|s| s.to_ascii_lowercase()) {
            Some(ref v) if v == "plain" || v == "text" || v == "pretty" => {
                false
            }
            Some(ref v) if v == "json" || v == "structured" => true,
            _ => false, // default to plain/text if not specified
        };
    let log_level =
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    let tracing_config =
        TracingConfig::from_env("oprc-gateway", &log_level, json_format);
    setup_tracing(tracing_config).expect("failed to setup tracing");

    // Initialize OTLP metrics exporter only if env provided
    let _ = oprc_observability::init_otlp_metrics_if_configured("oprc-gateway")
        .map_err(|e| tracing::warn!(error=%e, "failed to init otel metrics exporter"));
    let z_config = oprc_zenoh::OprcZenohConfig::init_from_env()?;

    let session = zenoh::open(z_config.create_zenoh()).await?;
    let mut router = handler::build_router(
        session,
        std::time::Duration::from_millis(config.request_timeout_ms),
    );
    let otel_metrics = Arc::new(handler::init_otel_metrics());
    router = router
        .layer(axum::middleware::from_fn(handler::otel_metrics))
        .layer(axum::Extension(otel_metrics));
    router = router.layer(axum::extract::DefaultBodyLimit::max(
        config.max_payload_bytes,
    ));
    router = router.layer(axum::Extension(config.retry_attempts)).layer(
        axum::Extension(std::time::Duration::from_millis(
            config.retry_backoff_ms,
        )),
    );
    let listener =
        TcpListener::bind(format!("0.0.0.0:{}", config.http_port)).await?;
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
