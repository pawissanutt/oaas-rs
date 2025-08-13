use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing::info;

pub async fn run_http_server(addr: SocketAddr) -> anyhow::Result<()> {
    let app = Router::new().route("/healthz", get(|| async { "ok" }));

    info!("CRM HTTP listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}
