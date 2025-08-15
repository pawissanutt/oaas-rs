use std::net::SocketAddr;

use kube::Client;
use tokio::{task::JoinHandle, try_join};

use crate::{
    config::CrmConfig,
    controller::run_controller,
    grpc::build_grpc_routes,
    web::run_http_server_with_grpc,
};

/// Compute the HTTP bind address based on config.
pub fn compute_http_addr(cfg: &CrmConfig) -> SocketAddr {
    ([0, 0, 0, 0], cfg.http_port).into()
}

/// Spawn the Kubernetes controller loop.
pub fn spawn_controller(client: Client) -> JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { run_controller(client).await })
}

/// Spawn the HTTP server that embeds gRPC routes on the provided address.
pub fn spawn_http_with_grpc(
    addr: SocketAddr,
    client: Client,
    k8s_namespace: String,
) -> JoinHandle<anyhow::Result<()>> {
    let grpc_routes = build_grpc_routes(client, k8s_namespace);
    tokio::spawn(async move { run_http_server_with_grpc(addr, grpc_routes).await })
}

/// Start both controller and HTTP+gRPC services and wait until either finishes.
pub async fn run_all(client: Client, cfg: CrmConfig) -> anyhow::Result<()> {
    let http_addr = compute_http_addr(&cfg);

    let controller = spawn_controller(client.clone());
    let http = spawn_http_with_grpc(http_addr, client, cfg.k8s_namespace.clone());

    let (c_res, h_res) = try_join!(controller, http)?;
    c_res?;
    h_res?;
    Ok(())
}
