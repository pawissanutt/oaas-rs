use chrono::Utc;
use oprc_cp_storage::{
    PackageFilter, PackageStorage, memory::MemoryPackageStorage,
};
use oprc_models::{OPackage, PackageMetadata};

#[tokio::test]
async fn memory_storage_crud() {
    let storage = MemoryPackageStorage::new();
    let package = OPackage {
        name: "test-package".to_string(),
        version: Some("1.0.0".to_string()),
        classes: vec![],
        functions: vec![],
        dependencies: vec![],
        deployments: vec![],
        metadata: PackageMetadata {
            author: Some("test-author".to_string()),
            description: Some("Test package".to_string()),
            tags: vec!["test".to_string()],
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        },
    };
    storage.store_package(&package).await.unwrap();
    let retrieved = storage.get_package("test-package").await.unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "test-package");
    let filter = PackageFilter {
        name_pattern: None,
        author: None,
        tags: vec![],
    };
    let packages = storage.list_packages(filter).await.unwrap();
    assert_eq!(packages.len(), 1);
    storage.delete_package("test-package").await.unwrap();
    let retrieved = storage.get_package("test-package").await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn package_filtering() {
    let storage = MemoryPackageStorage::new();
    let package1 = OPackage {
        name: "web-app".to_string(),
        version: Some("1.0.0".to_string()),
        classes: vec![],
        functions: vec![],
        dependencies: vec![],
        deployments: vec![],
        metadata: PackageMetadata {
            author: Some("test-author".to_string()),
            description: Some("Web application".to_string()),
            tags: vec!["web".to_string()],
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        },
    };
    let package2 = OPackage {
        name: "api-service".to_string(),
        version: Some("1.0.0".to_string()),
        classes: vec![],
        functions: vec![],
        dependencies: vec![],
        deployments: vec![],
        metadata: PackageMetadata {
            author: Some("test-author".to_string()),
            description: Some("API service".to_string()),
            tags: vec!["api".to_string()],
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        },
    };
    storage.store_package(&package1).await.unwrap();
    storage.store_package(&package2).await.unwrap();
    let filter = PackageFilter {
        name_pattern: None,
        author: None,
        tags: vec![],
    };
    let packages = storage.list_packages(filter).await.unwrap();
    assert_eq!(packages.len(), 2);
    assert_eq!(packages[0].name, "web-app");
    let filter = PackageFilter {
        name_pattern: Some("web".to_string()),
        author: None,
        tags: vec![],
    };
    let packages = storage.list_packages(filter).await.unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, "web-app");
}
