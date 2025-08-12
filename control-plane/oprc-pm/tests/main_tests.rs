use anyhow::Result;
use oprc_cp_storage::{
    PackageFilter,
    traits::{DeploymentStorage, PackageStorage, StorageFactory},
};
use oprc_models::DeploymentFilter;
use oprc_pm::{
    config::AppConfig,
    crm::CrmManager,
    server::ApiServer,
    services::{DeploymentService, PackageService},
    storage::create_storage_factory,
};
use serial_test::serial;
use std::{sync::Arc, time::Duration};
use tokio::time::timeout;

#[tokio::test]
#[serial]
async fn test_application_startup_with_memory_storage() -> Result<()> {
    // Set test environment variables for memory storage
    unsafe {
        std::env::set_var("STORAGE_TYPE", "memory");
        std::env::set_var("SERVER_HOST", "127.0.0.1");
        std::env::set_var("SERVER_PORT", "0"); // Random port
        std::env::set_var("CRM_DEFAULT_URL", "http://localhost:8081");
    }

    // Test configuration loading
    let config = AppConfig::load_from_env()?;

    assert!(matches!(
        config.storage().storage_type,
        oprc_pm::config::StorageType::Memory
    ));
    assert_eq!(config.server().host, "127.0.0.1");
    assert_eq!(config.server().port, 0);

    // Test storage factory creation
    let storage_config = config.storage();
    let storage_factory = create_storage_factory(&storage_config).await?;

    let package_storage = Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        Arc::new(storage_factory.create_deployment_storage());

    // Verify storage is working
    let empty_package_filter = PackageFilter {
        name_pattern: None,
        author: None,
        tags: vec![],
        disabled: None,
    };
    let empty_deployment_filter = DeploymentFilter {
        package_name: None,
        class_key: None,
        target_env: None,
        condition: None,
    };

    assert!(
        package_storage
            .list_packages(empty_package_filter)
            .await?
            .is_empty()
    );
    assert!(
        deployment_storage
            .list_deployments(empty_deployment_filter)
            .await?
            .is_empty()
    );

    // Test CRM manager creation
    let crm_config = config.crm();
    let crm_manager = Arc::new(CrmManager::new(crm_config)?);

    // Verify CRM manager has test clusters
    let clusters = crm_manager.list_clusters().await;
    assert!(!clusters.is_empty());

    // Test services creation
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
    ));

    let package_service = Arc::new(PackageService::new(
        package_storage.clone(),
        deployment_service.clone(),
    ));

    // Test API server creation (but don't start it)
    let server_config = config.server();
    let _api_server = ApiServer::new(
        package_service,
        deployment_service,
        crm_manager,
        server_config,
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_application_startup_with_etcd_storage() -> Result<()> {
    // Set test environment variables for etcd storage
    unsafe {
        std::env::set_var("STORAGE_TYPE", "etcd");
        std::env::set_var("ETCD_ENDPOINTS", "http://localhost:2379");
        std::env::set_var("SERVER_HOST", "127.0.0.1");
        std::env::set_var("SERVER_PORT", "0");
        std::env::set_var("CRM_DEFAULT_URL", "http://localhost:8081");
    }

    // Test configuration loading
    let config = AppConfig::load_from_env()?;

    assert!(matches!(
        config.storage().storage_type,
        oprc_pm::config::StorageType::Etcd
    ));
    if let Some(etcd_config) = &config.storage().etcd {
        assert!(!etcd_config.endpoints.is_empty());
    }

    // Test storage factory creation (this will fail if etcd is not available, which is expected)
    let storage_config = config.storage();

    // Use timeout to avoid hanging if etcd is not available
    let storage_factory_result = timeout(
        Duration::from_secs(5),
        create_storage_factory(&storage_config),
    )
    .await;

    match storage_factory_result {
        Ok(Ok(_storage_factory)) => {
            // If etcd is available, test that storage creation works
            // This is optional since etcd might not be running in all test environments
        }
        Ok(Err(_)) | Err(_) => {
            // Expected if etcd is not available - this is not a test failure
            // We're mainly testing that configuration parsing works correctly
        }
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_config_validation() -> Result<()> {
    // Test missing required configuration
    unsafe {
        std::env::remove_var("STORAGE_TYPE");
        std::env::remove_var("SERVER_HOST");
        std::env::remove_var("SERVER_PORT");
        std::env::remove_var("CRM_DEFAULT_URL");
    }

    // This should still work with defaults, but test that we get expected defaults
    let config = AppConfig::load_from_env()?;

    // Test that defaults are applied correctly
    assert_eq!(config.server().host, "0.0.0.0"); // Default host
    assert_eq!(config.server().port, 8080); // Default port
    assert!(matches!(
        config.storage().storage_type,
        oprc_pm::config::StorageType::Memory
    )); // Default storage

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_config_defaults() -> Result<()> {
    // Set minimal required configuration
    unsafe {
        // Clear conflicting env so defaults are applied
        std::env::remove_var("SERVER_HOST");
        std::env::remove_var("SERVER_PORT");
        std::env::remove_var("CRM_DEFAULT_URL");
        std::env::set_var("STORAGE_TYPE", "memory");
    }

    let config = AppConfig::load_from_env()?;

    // Test that defaults are applied correctly
    assert_eq!(config.server().host, "0.0.0.0"); // Default host
    assert_eq!(config.server().port, 8080); // Default port
    assert!(matches!(
        config.storage().storage_type,
        oprc_pm::config::StorageType::Memory
    ));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_crm_manager_initialization() -> Result<()> {
    // Set up test clusters configuration
    unsafe {
        // Ensure prior tests don't leave invalid SERVER_PORT
        std::env::set_var("SERVER_PORT", "8080");
        std::env::remove_var("SERVER_HOST");
        std::env::set_var("STORAGE_TYPE", "memory");
        std::env::set_var("CRM_DEFAULT_URL", "http://localhost:8080");
    }

    let config = AppConfig::load_from_env()?;
    let crm_config = config.crm();
    let crm_manager = CrmManager::new(crm_config)?;

    // Test that clusters are configured correctly
    let clusters = crm_manager.list_clusters().await;
    assert!(!clusters.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_storage_factory_memory() -> Result<()> {
    unsafe {
        std::env::set_var("STORAGE_TYPE", "memory");
    }

    let config = AppConfig::load_from_env()?;
    let storage_config = config.storage();

    let storage_factory = create_storage_factory(&storage_config).await?;

    // Test that we can create both storage types
    let package_storage = storage_factory.create_package_storage();
    let deployment_storage = storage_factory.create_deployment_storage();

    // Verify they work
    let empty_package_filter = PackageFilter {
        name_pattern: None,
        author: None,
        tags: vec![],
        disabled: None,
    };
    let empty_deployment_filter = DeploymentFilter {
        package_name: None,
        class_key: None,
        target_env: None,
        condition: None,
    };

    assert!(
        package_storage
            .list_packages(empty_package_filter)
            .await?
            .is_empty()
    );
    assert!(
        deployment_storage
            .list_deployments(empty_deployment_filter)
            .await?
            .is_empty()
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_server_config_parsing() -> Result<()> {
    // Test various server configurations
    unsafe {
        std::env::set_var("SERVER_HOST", "192.168.1.100");
        std::env::set_var("SERVER_PORT", "9090");
    }

    let config = AppConfig::load_from_env()?;
    let server_config = config.server();

    assert_eq!(server_config.host, "192.168.1.100");
    assert_eq!(server_config.port, 9090);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_invalid_port_configuration() -> Result<()> {
    unsafe {
        std::env::set_var("SERVER_PORT", "invalid_port");
    }

    let result = AppConfig::load_from_env();
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_application_component_integration() -> Result<()> {
    // Set up complete test environment
    unsafe {
        std::env::set_var("STORAGE_TYPE", "memory");
        std::env::set_var("SERVER_HOST", "127.0.0.1");
        std::env::set_var("SERVER_PORT", "0");
        std::env::set_var("CRM_DEFAULT_URL", "http://localhost:8082");
    }

    // Load configuration
    let config = AppConfig::load_from_env()?;

    // Create storage factory
    let storage_factory = create_storage_factory(&config.storage()).await?;
    let package_storage = Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        Arc::new(storage_factory.create_deployment_storage());

    // Create CRM manager
    let crm_manager = Arc::new(CrmManager::new(config.crm())?);

    // Create services
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
    ));

    let package_service = Arc::new(PackageService::new(
        package_storage.clone(),
        deployment_service.clone(),
    ));

    // Verify all components can work together
    let clusters = crm_manager.list_clusters().await;
    assert!(!clusters.is_empty());

    let packages = package_service
        .list_packages(oprc_pm::models::PackageFilter::default())
        .await?;
    assert!(packages.is_empty());

    let deployments = deployment_service
        .list_deployments(oprc_pm::models::DeploymentFilter::default())
        .await?;
    assert!(deployments.is_empty());

    Ok(())
}

/// Cleanup function to reset environment variables after tests
fn cleanup_env() {
    let env_vars = [
        // Unprefixed vars used by AppConfig
        "STORAGE_TYPE",
        "ETCD_ENDPOINTS",
        "SERVER_HOST",
        "SERVER_PORT",
        "CRM_DEFAULT_URL",
        // Prefixed vars (legacy/leftovers)
        "OPRC_PM_STORAGE_TYPE",
        "OPRC_PM_ETCD_ENDPOINTS",
        "OPRC_PM_SERVER_HOST",
        "OPRC_PM_SERVER_PORT",
        "OPRC_PM_CRM_DEFAULT_URL",
    ];

    unsafe {
        for var in &env_vars {
            std::env::remove_var(var);
        }
    }
}

#[tokio::test]
#[serial]
async fn test_cleanup() {
    cleanup_env();
}
