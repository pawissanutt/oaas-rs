use anyhow::Result;
use clap::Command;
use oprc_observability::{TracingConfig, setup_tracing};
use oprc_pm::bootstrap::build_api_server_from_env;
use std::env;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Setup tracing with configurable format
    let json_format = env::var("LOG_FORMAT")
        .unwrap_or_else(|_| "plain".to_string())
        .to_lowercase()
        == "json";
    let log_level = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    let tracing_config =
        TracingConfig::from_env("oprc-pm", &log_level, json_format);
    setup_tracing(tracing_config).expect("Failed to setup tracing");

    // Initialize OTLP metrics exporter if configured
    if let Err(e) =
        oprc_observability::init_otlp_metrics_if_configured("oprc-pm")
    {
        tracing::warn!(error = %e, "Failed to initialize OTLP metrics exporter");
    }

    let _matches = Command::new("oprc-pm")
        .about("OaaS Package Manager")
        .version("0.1.0")
        .get_matches();

    info!("Loading configuration from environment variables...");

    let server = build_api_server_from_env().await?;

    info!("Starting Package Manager API server...");
    if let Err(e) = server.serve().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
