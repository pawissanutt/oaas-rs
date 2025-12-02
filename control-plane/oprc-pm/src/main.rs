use anyhow::Result;
use clap::Command;
use oprc_cp_storage::StorageFactory;
use oprc_observability::{TracingConfig, setup_tracing};
use oprc_pm::{
    config::AppConfig,
    crm::CrmManager,
    server::ApiServer,
    services::{DeploymentService, PackageService},
    storage::create_storage_factory,
};
use std::{env, sync::Arc};
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
    let config = AppConfig::load_from_env()?;

    info!("Starting Package Manager with environment-based config");

    // Create storage factory
    let storage_config = config.storage();
    let storage_factory = create_storage_factory(&storage_config).await?;
    let package_storage = Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        Arc::new(storage_factory.create_deployment_storage());
    // Create CRM manager
    let crm_config = config.crm();
    let crm_manager = Arc::new(CrmManager::new(crm_config)?);

    // Create services
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
        config.deployment_policy(),
    ));

    let package_service = Arc::new(PackageService::new(
        package_storage.clone(),
        deployment_service.clone(),
        config.deployment_policy(),
    ));

    // Create and start the API server
    let server_config = config.server();
    let server = ApiServer::new(
        package_service,
        deployment_service,
        crm_manager,
        server_config,
    );

    info!("Starting Package Manager API server...");
    if let Err(e) = server.serve().await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
