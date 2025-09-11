use anyhow::Result;
use std::sync::Arc;

use crate::{
    config::AppConfig,
    crm::CrmManager,
    server::ApiServer,
    services::{DeploymentService, PackageService},
    storage::create_storage_factory,
};
use oprc_cp_storage::traits::StorageFactory;

/// Build a fully-wired ApiServer from environment variables.
/// Mirrors the logic in bin/main and is useful for tests and embedding.
pub async fn build_api_server_from_env() -> Result<ApiServer> {
    let config = AppConfig::load_from_env()?;

    // Storage factory and storages
    let storage_config = config.storage();
    let storage_factory = create_storage_factory(&storage_config).await?;
    let package_storage = Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        Arc::new(storage_factory.create_deployment_storage());

    // CRM
    let crm_manager = Arc::new(CrmManager::new(config.crm())?);

    // Services
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

    // Server
    let server_config = config.server();
    Ok(ApiServer::new(
        package_service,
        deployment_service,
        crm_manager,
        server_config,
    ))
}
