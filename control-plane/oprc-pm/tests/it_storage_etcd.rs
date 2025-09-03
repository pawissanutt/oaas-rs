#![cfg(feature = "etcd")]

use anyhow::Result;
use oprc_cp_storage::traits::{PackageStorage, StorageFactory};
use oprc_models::{
    enums::FunctionType,
    package::{
        FunctionBinding, OClass, OFunction, OPackage, PackageMetadata,
        StateSpecification,
    },
};
use oprc_pm::config::AppConfig;
use oprc_pm::crm::CrmManager;
use oprc_pm::services::{DeploymentService, PackageService};
use oprc_pm::storage::create_storage_factory;
use std::sync::Arc;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

fn sample_package(name: &str) -> OPackage {
    OPackage {
        name: name.to_string(),
        version: Some("0.0.1".to_string()),
        disabled: false,
        metadata: PackageMetadata {
            author: Some("test".into()),
            description: None,
            tags: vec![],
            created_at: None,
            updated_at: None,
        },
        classes: vec![OClass {
            key: "klass".into(),
            description: None,
            state_spec: Some(StateSpecification::default()),
            function_bindings: vec![FunctionBinding {
                name: "f1".into(),
                function_key: "f1".into(),
                ..Default::default()
            }],
            disabled: false,
        }],
        functions: vec![OFunction {
            key: "f1".into(),
            function_type: FunctionType::Custom,
            description: None,
            provision_config: None,
            config: Default::default(),
        }],
        dependencies: vec![],
        // Keep deployments empty to avoid contacting CRM in this storage-focused test
        deployments: vec![],
    }
}

#[tokio::test]
async fn pm_can_bootstrap_with_etcd_storage_and_do_crud() -> Result<()> {
    // Start etcd container
    let image = GenericImage::new("bitnami/etcd", "3.5")
        .with_exposed_port(2379.tcp())
        .with_wait_for(WaitFor::message_on_either_std(
            "grpc service status changed",
        ))
        .with_env_var("ALLOW_NONE_AUTHENTICATION", "yes")
        .with_env_var("ETCD_ADVERTISE_CLIENT_URLS", "http://127.0.0.1:2379");
    let container: ContainerAsync<GenericImage> = image.start().await?;
    let host_port = container.get_host_port_ipv4(2379.tcp()).await?;

    // Prepare AppConfig via env (AppConfig::load_from_env uses envconfig)
    unsafe {
        std::env::set_var("OPRC_PM_STORAGE_TYPE", "etcd");
        std::env::set_var(
            "OPRC_PM_ETCD_ENDPOINTS",
            &format!("127.0.0.1:{}", host_port),
        );
        std::env::set_var("OPRC_PM_ETCD_KEY_PREFIX", "/oaas/pm-test");
        std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:18080");
    }

    let config = AppConfig::load_from_env()?;
    let storage_cfg = config.storage();
    let factory = create_storage_factory(&storage_cfg).await?;

    let package_storage = Arc::new(factory.create_package_storage());
    let deployment_storage = Arc::new(factory.create_deployment_storage());

    // CRM manager can be created without contacting a real CRM since we won't call it here
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

    // Exercise CRUD through service layer hitting etcd storage underneath
    let pkg = sample_package("pkg-pm");
    let id = package_service.create_package(pkg.clone()).await?;
    assert_eq!(id.0.as_str(), "pkg-pm");

    let fetched = package_service
        .get_package("pkg-pm")
        .await?
        .expect("present");
    assert_eq!(fetched.name, "pkg-pm");

    // Direct storage check
    assert!(package_storage.package_exists("pkg-pm").await?);
    package_storage.delete_package("pkg-pm").await?;
    assert!(!package_storage.package_exists("pkg-pm").await?);

    Ok(())
}
