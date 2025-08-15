use chrono::Utc;
use oprc_cp_storage::DeploymentStorage;
use oprc_cp_storage::memory::MemoryDeploymentStorage;
use oprc_models::{DeploymentCondition, NfrRequirements, OClassDeployment};

fn make_deployment(key: &str) -> OClassDeployment {
    OClassDeployment {
        key: key.to_string(),
        package_name: "pkg".into(),
        class_key: "cls".into(),
        target_env: "dev".into(),
        target_clusters: vec!["c1".into(), "c2".into()],
        nfr_requirements: NfrRequirements::default(),
        functions: vec![],
        condition: DeploymentCondition::Pending,
    odgm: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[tokio::test]
async fn test_cluster_mapping_crud() {
    let storage = MemoryDeploymentStorage::new();
    let dep = make_deployment("dep-x");
    storage.store_deployment(&dep).await.unwrap();

    storage
        .save_cluster_mapping("dep-x", "c1", "unit-1")
        .await
        .unwrap();
    storage
        .save_cluster_mapping("dep-x", "c2", "unit-2")
        .await
        .unwrap();

    let map = storage.get_cluster_mappings("dep-x").await.unwrap();
    assert_eq!(map.get("c1").unwrap(), "unit-1");
    assert_eq!(map.get("c2").unwrap(), "unit-2");

    storage.remove_cluster_mappings("dep-x").await.unwrap();
    let map = storage.get_cluster_mappings("dep-x").await.unwrap();
    assert!(map.is_empty());
}
