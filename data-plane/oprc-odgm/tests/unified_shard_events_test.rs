use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use oprc_dp_storage::backends::memory::MemoryStorage;
use oprc_dp_storage::StorageConfig;
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::unified::ShardError;
use oprc_odgm::shard::unified::{ObjectUnifiedShard, UnifiedObjectShard};
use oprc_odgm::shard::{ObjectEntry, ObjectVal};
use oprc_pb::{InvocationRoute, ObjectEvent, ValType};

fn create_test_metadata() -> ShardMetadata {
    ShardMetadata {
        id: 1,
        collection: "test_collection".to_string(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "memory".to_string(),
        options: HashMap::new(),
        invocations: InvocationRoute::default(),
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}

fn create_test_object_entry(data: &str) -> ObjectEntry {
    let mut value = BTreeMap::new();
    value.insert(
        1,
        ObjectVal {
            data: data.as_bytes().to_vec(),
            r#type: ValType::Byte,
        },
    );

    ObjectEntry {
        last_updated: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        value,
        event: Some(ObjectEvent::default()),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_object_without_events() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();

    // Create replication layer with the same storage instance
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard =
        ObjectUnifiedShard::new_minimal(metadata, storage, replication).await?;

    shard.initialize().await?;

    // Test set operation works without events
    let test_entry = create_test_object_entry("test_data");
    shard.set_object(123, test_entry.clone()).await?;

    // Verify object was stored
    let retrieved = shard.get_object(123).await?;
    assert!(retrieved.is_some(), "Object should be stored");

    let retrieved_entry = retrieved.unwrap();
    assert_eq!(retrieved_entry.value, test_entry.value);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delete_object_without_events() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();

    // Create replication layer with the same storage instance
    let replication = NoReplication::new(storage.clone());

    // Create unified shard without event manager
    let shard =
        ObjectUnifiedShard::new_minimal(metadata, storage, replication).await?;

    shard.initialize().await?;

    // First set an object
    let test_entry = create_test_object_entry("test_data_to_delete");
    shard.set_object(456, test_entry).await?;

    // Verify it exists
    let retrieved = shard.get_object(456).await?;
    assert!(retrieved.is_some(), "Object should exist before deletion");

    // Test delete operation
    shard.delete_object(&456).await?;

    // Verify it's deleted
    let retrieved_after_delete = shard.get_object(456).await?;
    assert!(retrieved_after_delete.is_none(), "Object should be deleted");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_update_object_operation() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard =
        ObjectUnifiedShard::new_minimal(metadata, storage, replication).await?;

    shard.initialize().await?;

    // First set an object
    let initial_entry = create_test_object_entry("initial_data");
    shard.set_object(789, initial_entry).await?;

    // Update the object
    let updated_entry = create_test_object_entry("updated_data");
    shard.set_object(789, updated_entry.clone()).await?;

    // Verify the update
    let retrieved = shard.get_object(789).await?;
    assert!(retrieved.is_some(), "Updated object should exist");

    let retrieved_entry = retrieved.unwrap();
    assert_eq!(retrieved_entry.value, updated_entry.value);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_batch_operations() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard =
        ObjectUnifiedShard::new_minimal(metadata, storage, replication).await?;

    shard.initialize().await?;

    // Test batch set operations
    let entries = vec![
        (100, create_test_object_entry("batch_data_1")),
        (101, create_test_object_entry("batch_data_2")),
        (102, create_test_object_entry("batch_data_3")),
    ];

    shard.batch_set_objects(entries).await?;

    // Verify all objects were stored
    for object_id in [100, 101, 102] {
        let retrieved = shard.get_object(object_id).await?;
        assert!(
            retrieved.is_some(),
            "Batch set object {} should exist",
            object_id
        );
    }

    // Test batch delete operations
    let keys_to_delete = vec![100, 101, 102];
    shard.batch_delete_objects(keys_to_delete).await?;

    // Verify all objects were deleted
    for object_id in [100, 101, 102] {
        let retrieved = shard.get_object(object_id).await?;
        assert!(
            retrieved.is_none(),
            "Batch deleted object {} should not exist",
            object_id
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_count_and_scan_operations() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard =
        ObjectUnifiedShard::new_minimal(metadata, storage, replication).await?;

    shard.initialize().await?;

    // Initially should be empty
    let initial_count = shard.count_objects().await?;
    assert_eq!(initial_count, 0, "Initial count should be 0");

    // Add some objects
    for i in 1..=5 {
        let entry = create_test_object_entry(&format!("test_data_{}", i));
        shard.set_object(i, entry).await?;
    }

    // Test count
    let count_after_insert = shard.count_objects().await?;
    assert_eq!(
        count_after_insert, 5,
        "Count should be 5 after inserting 5 objects"
    );

    // Test scan (Note: scan may not return all objects due to prefix filtering)
    let scanned_objects = shard.scan_objects(None).await?;
    assert!(
        !scanned_objects.is_empty(),
        "Scan should return some objects"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_unified_shard_trait_object() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard =
        ObjectUnifiedShard::new_minimal(metadata, storage, replication).await?;

    shard.initialize().await?;

    // Test that we can use it as a trait object
    let trait_object: Box<dyn UnifiedObjectShard> = Box::new(shard);

    // Test operations through trait
    let test_entry = create_test_object_entry("trait_test_data");
    trait_object.set_object(999, test_entry).await?;

    let retrieved = trait_object.get_object(999).await?;
    assert!(retrieved.is_some(), "Object set through trait should exist");

    Ok(())
}
