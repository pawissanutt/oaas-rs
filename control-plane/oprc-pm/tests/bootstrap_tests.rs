#[cfg(test)]
mod simple_tests {
    use anyhow::Result;
    use oprc_pm::{
        config::AppConfig,
        crm::CrmManager,
        services::{DeploymentService, PackageService},
        storage::create_storage_factory,
    };
    use oprc_cp_storage::traits::StorageFactory;
    use std::sync::Arc;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_memory_storage_creation() -> Result<()> {
        unsafe {
            std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
        }

        let config = AppConfig::load_from_env()?;
        let storage_config = config.storage();
        let storage_factory = create_storage_factory(&storage_config).await?;
        
        let _package_storage = storage_factory.create_package_storage();
        let _deployment_storage = storage_factory.create_deployment_storage();

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_crm_manager_with_url() -> Result<()> {
        unsafe {
            std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:8080");
        }

        let config = AppConfig::load_from_env()?;
        let crm_config = config.crm();
        let _crm_manager = CrmManager::new(crm_config)?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_service_creation() -> Result<()> {
        unsafe {
            std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
            std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:8080");
        }

        let config = AppConfig::load_from_env()?;
        
        // Create storage
        let storage_factory = create_storage_factory(&config.storage()).await?;
        let package_storage = Arc::new(storage_factory.create_package_storage());
        let deployment_storage = Arc::new(storage_factory.create_deployment_storage());

        // Create CRM manager
        let crm_manager = Arc::new(CrmManager::new(config.crm())?);

        // Create services
        let deployment_service = Arc::new(DeploymentService::new(
            deployment_storage,
            crm_manager.clone(),
        ));

        let _package_service = Arc::new(PackageService::new(
            package_storage,
            deployment_service,
        ));

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_configuration_loads() -> Result<()> {
        let config = AppConfig::load_from_env()?;
        
        // Just test that config loads without error
        let _server_config = config.server();
        let _storage_config = config.storage();
        let _crm_config = config.crm();

        Ok(())
    }

    #[tokio::test]
    #[serial] 
    async fn test_complete_application_bootstrap() -> Result<()> {
        // Set up a complete test environment
        unsafe {
            std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
            std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:8081");
        }

        // This mimics what main.rs does
        let config = AppConfig::load_from_env()?;

        // Create storage factory
        let storage_factory = create_storage_factory(&config.storage()).await?;
        let package_storage = Arc::new(storage_factory.create_package_storage());
        let deployment_storage = Arc::new(storage_factory.create_deployment_storage());

        // Create CRM manager
        let crm_manager = Arc::new(CrmManager::new(config.crm())?);

        // Create services  
        let deployment_service = Arc::new(DeploymentService::new(
            deployment_storage,
            crm_manager.clone(),
        ));

        let package_service = Arc::new(PackageService::new(
            package_storage,
            deployment_service.clone(),
        ));

        // Create API server configuration (don't actually start it)
        let server_config = config.server();
        
        // This would be where we create the API server in main.rs
        let _api_server = oprc_pm::server::ApiServer::new(
            package_service,
            deployment_service,
            crm_manager,
            server_config,
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn cleanup_env() {
        unsafe {
            std::env::remove_var("OPRC_PM_STORAGE_TYPE");
            std::env::remove_var("OPRC_PM_CRM_DEFAULT_URL");
            std::env::remove_var("OPRC_PM_SERVER_HOST");
            std::env::remove_var("OPRC_PM_SERVER_PORT");
        }
    }
}
