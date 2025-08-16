#[cfg(test)]
mod bootstrap_basic_tests {
    use anyhow::Result;
    use oprc_cp_storage::traits::StorageFactory;
    use oprc_pm::{
        config::AppConfig,
        crm::CrmManager,
        services::{DeploymentService, PackageService},
        storage::create_storage_factory,
    };
    use serial_test::serial;
    use std::sync::Arc;

    #[tokio::test]
    #[serial]
    async fn memory_storage_creation() -> Result<()> {
        unsafe {
            std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
        }
        let config = AppConfig::load_from_env()?;
        let storage_factory = create_storage_factory(&config.storage()).await?;
        let _package_storage = storage_factory.create_package_storage();
        let _deployment_storage = storage_factory.create_deployment_storage();
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn crm_manager_with_url() -> Result<()> {
        unsafe {
            std::env::set_var(
                "OPRC_PM_CRM_DEFAULT_URL",
                "http://localhost:8080",
            );
        }
        let config = AppConfig::load_from_env()?;
        let _crm_manager = CrmManager::new(config.crm())?;
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn service_creation() -> Result<()> {
        unsafe {
            std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
            std::env::set_var(
                "OPRC_PM_CRM_DEFAULT_URL",
                "http://localhost:8080",
            );
        }
        let config = AppConfig::load_from_env()?;
        let storage_factory = create_storage_factory(&config.storage()).await?;
        let package_storage =
            Arc::new(storage_factory.create_package_storage());
        let deployment_storage =
            Arc::new(storage_factory.create_deployment_storage());
        let crm_manager = Arc::new(CrmManager::new(config.crm())?);
        let deployment_service = Arc::new(DeploymentService::new(
            deployment_storage,
            crm_manager.clone(),
            oprc_pm::config::DeploymentPolicyConfig {
                max_retries: 0,
                rollback_on_partial: false,
                package_delete_cascade: false,
            },
        ));
        let _package_service = Arc::new(PackageService::new(
            package_storage,
            deployment_service,
            oprc_pm::config::DeploymentPolicyConfig {
                max_retries: 0,
                rollback_on_partial: false,
                package_delete_cascade: false,
            },
        ));
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn configuration_loads() -> Result<()> {
        let config = AppConfig::load_from_env()?;
        let _ = config.server();
        let _ = config.storage();
        let _ = config.crm();
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn complete_application_bootstrap() -> Result<()> {
        unsafe {
            std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
            std::env::set_var(
                "OPRC_PM_CRM_DEFAULT_URL",
                "http://localhost:8081",
            );
        }
        let config = AppConfig::load_from_env()?;
        let storage_factory = create_storage_factory(&config.storage()).await?;
        let package_storage =
            Arc::new(storage_factory.create_package_storage());
        let deployment_storage =
            Arc::new(storage_factory.create_deployment_storage());
        let crm_manager = Arc::new(CrmManager::new(config.crm())?);
        let deployment_service = Arc::new(DeploymentService::new(
            deployment_storage,
            crm_manager.clone(),
            oprc_pm::config::DeploymentPolicyConfig {
                max_retries: 0,
                rollback_on_partial: false,
                package_delete_cascade: false,
            },
        ));
        let package_service = Arc::new(PackageService::new(
            package_storage,
            deployment_service.clone(),
            oprc_pm::config::DeploymentPolicyConfig {
                max_retries: 0,
                rollback_on_partial: false,
                package_delete_cascade: false,
            },
        ));
        let server_config = config.server();
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
