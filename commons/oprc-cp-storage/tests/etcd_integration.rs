#![cfg(feature = "etcd")]

use anyhow::Result;
use oprc_cp_storage::etcd::EtcdStorageFactory;
use oprc_cp_storage::traits::{
    DeploymentStorage, PackageStorage, StorageFactory,
};
use oprc_models::{
    deployment::OClassDeployment,
    enums::FunctionType,
    package::{
        FunctionBinding, OClass, OFunction, OPackage, PackageMetadata,
        StateSpecification,
    },
};
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
            function_bindings: vec![FunctionBinding::default()],
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
        deployments: vec![OClassDeployment {
            key: "dep1".into(),
            package_name: name.into(),
            class_key: "klass".into(),
            target_envs: vec!["dev".into()],
            ..Default::default()
        }],
    }
}

#[tokio::test]
async fn etcd_storage_crud_works() -> Result<()> {
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

    // Build etcd storage
    let endpoints = vec![format!("127.0.0.1:{}", host_port)];
    let factory = EtcdStorageFactory::new(endpoints, "/oaas/test".to_string());
    let connected = factory.connect().await?;
    let storage = connected.create_package_storage();

    // Perform CRUD
    let pkg = sample_package("pkg1");
    storage.store_package(&pkg).await?;
    assert!(storage.package_exists(&pkg.name).await?);

    let fetched = storage.get_package(&pkg.name).await?.expect("present");
    assert_eq!(fetched.name, pkg.name);

    let list = storage
        .list_packages(oprc_cp_storage::traits::PackageFilter {
            name_pattern: None,
            author: None,
            tags: vec![],
            disabled: None,
        })
        .await?;
    assert!(!list.is_empty());

    storage.delete_package(&pkg.name).await?;
    assert!(!storage.package_exists(&pkg.name).await?);

    Ok(())
}

#[tokio::test]
async fn etcd_deployment_cluster_mapping_works() -> Result<()> {
    let image = GenericImage::new("bitnami/etcd", "3.5")
        .with_exposed_port(2379.tcp())
        .with_wait_for(WaitFor::message_on_either_std(
            "grpc service status changed",
        ))
        .with_env_var("ALLOW_NONE_AUTHENTICATION", "yes")
        .with_env_var("ETCD_ADVERTISE_CLIENT_URLS", "http://127.0.0.1:2379");

    let container: ContainerAsync<GenericImage> = image.start().await?;
    let host_port = container.get_host_port_ipv4(2379.tcp()).await?;

    let endpoints = vec![format!("127.0.0.1:{}", host_port)];
    let factory = EtcdStorageFactory::new(endpoints, "/oaas/test".to_string());
    let connected = factory.connect().await?;
    let dep_storage = connected.create_deployment_storage();

    let deployment = OClassDeployment {
        key: "depA".into(),
        package_name: "pkgA".into(),
        class_key: "klass".into(),
        target_envs: vec!["dev".into()],
        ..Default::default()
    };
    dep_storage.store_deployment(&deployment).await?;

    dep_storage
        .save_cluster_mapping(&deployment.key, "c1", "id-1")
        .await?;
    dep_storage
        .save_cluster_mapping(&deployment.key, "c2", "id-2")
        .await?;
    let map = dep_storage.get_cluster_mappings(&deployment.key).await?;
    assert_eq!(map.get("c1"), Some(&"id-1".to_string()));
    assert_eq!(map.get("c2"), Some(&"id-2".to_string()));

    dep_storage.remove_cluster_mappings(&deployment.key).await?;
    let map2 = dep_storage.get_cluster_mappings(&deployment.key).await?;
    assert!(map2.is_empty());

    Ok(())
}
