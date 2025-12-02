use envconfig::Envconfig;
use kube::Client;
use oprc_crm::{config::CrmConfig, runtime};
use oprc_observability::{TracingConfig, setup_tracing};
use oprc_zenoh::OprcZenohConfig;
use std::env;
use std::sync::Arc;
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Setup tracing with configurable format
    let json_format = env::var("LOG_FORMAT")
        .unwrap_or_else(|_| "plain".to_string())
        .to_lowercase()
        == "json";
    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    let config = TracingConfig::from_env("oprc-crm", &log_level, json_format);
    setup_tracing(config).expect("Failed to setup tracing");

    // Initialize OTLP metrics exporter if configured
    if let Err(e) =
        oprc_observability::init_otlp_metrics_if_configured("oprc-crm")
    {
        tracing::warn!(error = %e, "Failed to initialize OTLP metrics exporter");
    }

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

    // Initialize Zenoh
    let zenoh_cfg = OprcZenohConfig::init_from_env()?;
    let session = zenoh::open(zenoh_cfg.create_zenoh())
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let zenoh = Arc::new(session);

    let client = Client::try_default().await?;
    runtime::run_all(client, cfg, zenoh).await
}
