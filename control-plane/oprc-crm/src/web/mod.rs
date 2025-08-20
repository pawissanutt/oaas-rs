use axum::{Router, routing::get};
use std::net::SocketAddr;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::info;

pub async fn run_http_server_with_grpc(
    addr: SocketAddr,
    grpc_routes: tonic::service::Routes,
) -> anyhow::Result<()> {
    // Expose both /health (preferred) and /healthz (legacy) for compatibility
    let http_router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/healthz", get(|| async { "ok" }));

    // Use gRPC service as fallback so /grpc routes are handled by tonic
    let app = http_router
        .fallback_service(grpc_routes)
        .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()));

    info!("CRM HTTP listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
