use axum::{Router, extract::Extension, routing::get};
use oprc_observability::{OtelMetrics, otel_metrics_middleware};
use std::{net::SocketAddr, sync::Arc};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

#[cfg(feature = "otel")]
use oprc_grpc::tracing::OtelGrpcServerLayer;

pub async fn run_http_server_with_grpc(
    addr: SocketAddr,
    grpc_routes: tonic::service::Routes,
) -> anyhow::Result<()> {
    // Initialize OTEL metrics
    let otel_metrics = Arc::new(OtelMetrics::new("oprc-crm"));

    // Expose both /health (preferred) and /healthz (legacy) for compatibility
    let http_router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/healthz", get(|| async { "ok" }));

    // Convert gRPC routes to axum router and optionally apply OTEL layer
    #[cfg(feature = "otel")]
    let grpc_router = grpc_routes
        .into_axum_router()
        .layer(OtelGrpcServerLayer::default());

    #[cfg(not(feature = "otel"))]
    let grpc_router = grpc_routes.into_axum_router();

    // Use gRPC service as fallback so /grpc routes are handled by tonic
    let app = http_router
        .fallback_service(grpc_router)
        .layer(axum::middleware::from_fn(otel_metrics_middleware))
        .layer(Extension(otel_metrics))
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    info!("CRM HTTP listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
