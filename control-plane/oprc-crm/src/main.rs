use envconfig::Envconfig;
use kube::Client;
use oprc_crm::{config::CrmConfig, init_tracing, runtime};
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing("info");

    let cfg = CrmConfig::init_from_env()?.apply_profile_defaults();
    info!(?cfg, "Starting CRM");

    let client = Client::try_default().await?;
    runtime::run_all(client, cfg).await
}
