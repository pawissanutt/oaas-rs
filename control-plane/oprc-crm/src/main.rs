use envconfig::Envconfig;
use kube::Client;
use oprc_crm::{config::CrmConfig, init_tracing, runtime};
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing("info");

    // Ensure rustls uses the aws-lc-rs provider explicitly.
    // This avoids runtime errors when no default provider is set.
    if let Err(e) = rustls::crypto::CryptoProvider::install_default(
        rustls::crypto::aws_lc_rs::default_provider(),
    ) {
        // It's fine if a compatible provider was already installed.
        tracing::debug!(
            ?e,
            "CryptoProvider already installed or incompatible; proceeding"
        );
    }

    let cfg = CrmConfig::init_from_env()?.apply_profile_defaults();
    info!(?cfg, "Starting CRM");

    let client = Client::try_default().await?;
    runtime::run_all(client, cfg).await
}
