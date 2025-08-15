use std::net::SocketAddr;

use envconfig::Envconfig;
use kube::Client;
use oprc_crm::{config::CrmConfig, controller::run_controller, init_tracing};
use tokio::try_join;
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing("info");

    let cfg = CrmConfig::init_from_env()?.apply_profile_defaults();
    info!(?cfg, "Starting CRM");

    let client = Client::try_default().await?;
    let controller_client = client.clone();

    let http_addr: SocketAddr = ([0, 0, 0, 0], cfg.http_port).into();

    let controller =
        tokio::spawn(async move { run_controller(controller_client).await });
    // Build a single axum server that embeds gRPC routes via tonic::service::Routes
    let grpc_routes = oprc_crm::grpc::build_grpc_routes(
        client.clone(),
        cfg.k8s_namespace.clone(),
    );
    let http = tokio::spawn(async move {
        // Bind to the HTTP port but serve both HTTP and gRPC on the same socket
        oprc_crm::web::run_http_server_with_grpc(http_addr, grpc_routes).await
    });

    // If any fails, bubble up
    let (c_res, h_res) = try_join!(controller, http)?;
    c_res?;
    h_res?;
    Ok(())
}
