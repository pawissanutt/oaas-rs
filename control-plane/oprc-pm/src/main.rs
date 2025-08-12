use anyhow::Result;
use clap::Command;
use oprc_cp_storage::StorageFactory;
use oprc_pm::{
    config::AppConfig,
    crm::CrmManager,
    server::ApiServer,
    services::{DeploymentService, PackageService},
    storage::create_storage_factory,
};
use std::sync::Arc;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .json()
        .init();

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
    let _runtime_storage = Arc::new(storage_factory.create_runtime_storage());

    // Create CRM manager
    let crm_config = config.crm();
    let crm_manager = Arc::new(CrmManager::new(crm_config)?);

    // Create services
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
    ));

    let package_service = Arc::new(PackageService::new(
        package_storage.clone(),
        deployment_service.clone(),
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
