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
    let grpc_addr: SocketAddr = ([0, 0, 0, 0], cfg.grpc_port).into();

    let controller =
        tokio::spawn(async move { run_controller(controller_client).await });
    let grpc_client = client.clone();
    let default_ns = cfg.k8s_namespace.clone();
    let grpc = tokio::spawn(async move {
        oprc_crm::grpc::run_grpc_server(grpc_addr, grpc_client, default_ns)
            .await
    });
    let http =
        tokio::spawn(
            async move { oprc_crm::web::run_http_server(http_addr).await },
        );

    // If any fails, bubble up
    let (c_res, g_res, h_res) = try_join!(controller, grpc, http)?;
    c_res?;
    g_res?;
    h_res?;
    Ok(())
}
