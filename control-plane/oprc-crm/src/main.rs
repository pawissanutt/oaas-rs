use envconfig::Envconfig;
use kube::Client;
use oprc_crm::{config::CrmConfig, runtime};
use oprc_observability::{TracingConfig, setup_tracing};
use std::env;
use tracing::info;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Setup tracing with configurable format
    let json_format = env::var("LOG_FORMAT")
        .unwrap_or_else(|_| "plain".to_string())
        .to_lowercase()
        == "json";

    let config = TracingConfig {
        service_name: "oprc-crm".to_string(),
        log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        json_format,
        otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
    };

    setup_tracing(config).expect("Failed to setup tracing");

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
