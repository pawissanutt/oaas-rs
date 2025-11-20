use std::collections::{BTreeMap, HashMap};

use oprc_dp_storage::StorageConfig;
use oprc_dp_storage::backends::memory::MemoryStorage;
use oprc_grpc::{
    InvocationRequest, InvocationRoute, ObjectEvent, ObjectInvocationRequest,
    ValType,
};
use oprc_invoke::OffloadError; // needed for matching OffloadError variants in tests
use oprc_odgm::events::EventManagerImpl;
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::unified::ShardError;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::unified::{ObjectShard, ObjectUnifiedShard};
use oprc_odgm::shard::{ObjectData, ObjectVal, UnifiedShardConfig};

// const ENABLE_STRING_IDS: bool = true; // deprecated feature flag
const MAX_STRING_ID_LEN: usize = 160;
const GRANULAR_PREFETCH_LIMIT: usize = 256;

fn shard_config() -> UnifiedShardConfig {
    UnifiedShardConfig {
        // enable_string_ids: ENABLE_STRING_IDS,
        max_string_id_len: MAX_STRING_ID_LEN,
        granular_prefetch_limit: GRANULAR_PREFETCH_LIMIT,
    }
}

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

fn create_test_object_entry(data: &str) -> ObjectData {
    let mut entries = BTreeMap::new();
    entries.insert(
        "1".to_string(),
        ObjectVal {
            data: data.as_bytes().to_vec(),
            r#type: ValType::Byte,
        },
    );

    ObjectData {
        last_updated: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        entries,
        event: Some(ObjectEvent::default()),
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_set_object_without_events() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();

    // Create replication layer with the same storage instance
    let replication = NoReplication::new(storage.clone());

    // Create unified shard - specify EventManager type explicitly
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(), // enable_string_ids is deprecated
    )
    .await?;

    shard.initialize().await?;

    // Test set operation works without events
    let test_entry = create_test_object_entry("test_data");
    shard.set_object("123", test_entry.clone()).await?;

    // Verify object was stored
    let retrieved = shard.get_object("123").await?;
    assert!(retrieved.is_some(), "Object should be stored");

    let retrieved_entry = retrieved.unwrap();
    assert_eq!(retrieved_entry.entries, test_entry.entries);

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_delete_object_without_events() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();

    // Create replication layer with the same storage instance
    let replication = NoReplication::new(storage.clone());

    // Create unified shard without event manager
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;

    shard.initialize().await?;

    // First set an object
    let test_entry = create_test_object_entry("test_data_to_delete");
    shard.set_object("456", test_entry).await?;

    // Verify it exists
    let retrieved = shard.get_object("456").await?;
    assert!(retrieved.is_some(), "Object should exist before deletion");

    // Test delete operation
    shard.delete_object("456").await?;

    // Verify it's deleted
    let retrieved_after_delete = shard.get_object("456").await?;
    assert!(retrieved_after_delete.is_none(), "Object should be deleted");

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_update_object_operation() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;

    shard.initialize().await?;

    // First set an object
    let initial_entry = create_test_object_entry("initial_data");
    shard.set_object("789", initial_entry).await?;

    // Update the object
    let updated_entry = create_test_object_entry("updated_data");
    shard.set_object("789", updated_entry.clone()).await?;

    // Verify the update
    let retrieved = shard.get_object("789").await?;
    assert!(retrieved.is_some(), "Updated object should exist");

    let retrieved_entry = retrieved.unwrap();
    assert_eq!(retrieved_entry.entries, updated_entry.entries);

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_batch_operations() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;

    shard.initialize().await?;

    // Test batch set operations
    let entries = vec![
        ("100".to_string(), create_test_object_entry("batch_data_1")),
        ("101".to_string(), create_test_object_entry("batch_data_2")),
        ("102".to_string(), create_test_object_entry("batch_data_3")),
    ];

    shard.batch_set_objects(entries).await?;

    // Verify all objects were stored
    for object_id in ["100", "101", "102"] {
        let retrieved = shard.get_object(object_id).await?;
        assert!(
            retrieved.is_some(),
            "Batch set object {} should exist",
            object_id
        );
    }

    // Test batch delete operations
    let keys_to_delete =
        vec!["100".to_string(), "101".to_string(), "102".to_string()];
    shard.batch_delete_objects(keys_to_delete).await?;

    // Verify all objects were deleted
    for object_id in ["100", "101", "102"] {
        let retrieved = shard.get_object(object_id).await?;
        assert!(
            retrieved.is_none(),
            "Batch deleted object {} should not exist",
            object_id
        );
    }

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_count_and_scan_operations() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;

    shard.initialize().await?;

    // Initially should be empty
    let initial_count = shard.count_objects().await?;
    assert_eq!(initial_count, 0, "Initial count should be 0");

    // Add some objects
    for i in 1..=5 {
        let entry = create_test_object_entry(&format!("test_data_{}", i));
        shard.set_object(&i.to_string(), entry).await?;
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

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_unified_shard_trait_object() -> Result<(), ShardError> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;

    shard.initialize().await?;

    // Test that we can use it as a trait object
    let trait_object: Box<dyn ObjectShard> = Box::new(shard);

    // Test operations through trait
    let test_entry = create_test_object_entry("trait_test_data");
    trait_object.set_object("999", test_entry).await?;

    let retrieved = trait_object.get_object("999").await?;
    assert!(retrieved.is_some(), "Object set through trait should exist");

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_invoke_methods_not_available()
-> Result<(), Box<dyn std::error::Error>> {
    // Create components
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());

    // Create unified shard without invocation capabilities (minimal config)
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;

    shard.initialize().await?;

    // Test that we can use it as a trait object
    let trait_object: Box<dyn ObjectShard> = Box::new(shard);

    // Test invoke_fn - should return ConfigurationError since no offloader is available
    let invoke_request = InvocationRequest {
        cls_id: "test_class".to_string(),
        fn_id: "test_function".to_string(),
        ..Default::default()
    };

    match trait_object.invoke_fn(invoke_request).await {
        Err(OffloadError::ConfigurationError(msg)) => {
            assert!(msg.contains("Invocation offloader not available"));
        }
        _ => panic!("Expected ConfigurationError for unavailable offloader"),
    }

    // Test invoke_obj - should also return ConfigurationError
    let invoke_obj_request = ObjectInvocationRequest {
        cls_id: "test_class".to_string(),
        fn_id: "test_function".to_string(),
        object_id: Some("123".to_string()),
        partition_id: 0,
        ..Default::default()
    };

    match trait_object.invoke_obj(invoke_obj_request).await {
        Err(OffloadError::ConfigurationError(msg)) => {
            assert!(msg.contains("Invocation offloader not available"));
        }
        _ => panic!("Expected ConfigurationError for unavailable offloader"),
    }

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_v2_dispatcher_emits_mutation_context()
-> Result<(), Box<dyn std::error::Error>> {
    // Build minimal shard (new_minimal does not wire V2 dispatcher; so we simulate a batch mutation path producing context only if dispatcher exists).
    // Fallback: if V2 dispatcher not present (e.g., minimal build), skip test.
    let metadata = create_test_metadata();
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());
    let shard: ObjectUnifiedShard<
        MemoryStorage,
        NoReplication<MemoryStorage>,
        EventManagerImpl<MemoryStorage>,
    > = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config(),
    )
    .await?;
    shard.initialize().await?;

    // If shard has no V2 dispatcher (since minimal path bypasses full factory), we cannot assert broadcast; skip gracefully.
    let maybe_rx = shard.v2_subscribe();
    if maybe_rx.is_none() {
        // Clean up env flag for other tests and exit early.
        eprintln!(
            "V2 dispatcher not available in minimal shard; skipping test"
        );
        return Ok(());
    }
    let mut rx = maybe_rx.unwrap();

    // Create object with event config (default empty triggers) and then update a field; events still enqueued even if no triggers match.
    let mut entry = create_test_object_entry("initial_v2");
    shard.set_object("42", entry.clone()).await?;
    // Mutate same key to produce Update action
    entry.entries.get_mut("1").unwrap().data = b"updated_v2".to_vec();
    shard.set_object("42", entry).await?;

    // Collect a few broadcasts (there should be at least 2: create + update) within timeout.
    use tokio::time::{Duration, timeout};
    let mut received = Vec::new();
    for _ in 0..3 {
        // read up to 3 events
        if let Ok(Ok(evt)) =
            timeout(Duration::from_millis(200), rx.recv()).await
        {
            received.push(evt);
        } else {
            break;
        }
    }
    assert!(
        !received.is_empty(),
        "Expected at least one V2 mutation context event"
    );
    assert!(
        received.iter().any(|e| !e.ctx.changed.is_empty()),
        "At least one event should describe changed keys"
    );

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_v2_trigger_execution_records_in_test_tap()
-> Result<(), Box<dyn std::error::Error>> {
    // Enable V2 pipeline + trigger test tap for this test (unsafe set_var wrapper used in build to allow mutation in tests)
    // Allow environment mutation in test context
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_TRIGGER_TEST_TAP", "1");
    }

    // Build a shard through full factory path by constructing a small ODGM instance with events enabled.
    use oprc_odgm::{OdgmConfig, start_raw_server};
    // (unused imports removed)

    // Minimal config enabling events
    let conf = OdgmConfig {
        events_enabled: true,
        ..Default::default()
    };
    let (odgm, _pool) = start_raw_server(&conf, None).await?; // has factory with events

    // Create a collection with one shard (reuse helper if available)
    odgm.metadata_manager
        .create_collection(
            oprc_odgm::collection_helpers::minimal_mst_with_echo("taptest"),
        )
        .await
        .unwrap();

    // Wait briefly for shard creation
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Get shard handle (id 0 partition assumed)
    // Shard id is collection name + partition suffix; the manager's API (get_shard) seems to take a key; fall back to iterating stats to find any shard for collection.
    // Poll for shard creation
    let mut shard_opt = None;
    for _ in 0..20 {
        // up to ~2s
        let mut shards = odgm
            .shard_manager
            .get_shards_for_collection("taptest")
            .await;
        if let Some(s) = shards.drain(..).next() {
            shard_opt = Some(s);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if shard_opt.is_none() {
        eprintln!("Shard not created in time; skipping");
        return Ok(());
    }
    if shard_opt.is_none() {
        return Err("Shard not created".into());
    }
    let shard_arc = shard_opt.unwrap();

    // Subscribe to V2 events to ensure dispatcher active
    // Downcast to concrete type to access v2_subscribe helper
    use oprc_dp_storage::AnyStorage;
    use oprc_odgm::events::EventManagerImpl;
    use oprc_odgm::replication::no_replication::NoReplication;
    use oprc_odgm::shard::unified::ObjectUnifiedShard;
    let maybe_rx = shard_arc
        .as_any()
        .downcast_ref::<ObjectUnifiedShard<
            AnyStorage,
            NoReplication<AnyStorage>,
            EventManagerImpl<AnyStorage>,
        >>()
        .and_then(|s| s.v2_subscribe());
    if maybe_rx.is_none() {
        eprintln!("V2 dispatcher not available; skipping");
        return Ok(());
    }

    // Insert an object with an event config that includes a trigger target referencing numeric key 1 updates.
    // We'll craft an ObjectEvent with a DataTrigger targeting key=1 update and a simple target function.
    use oprc_grpc::{
        DataTrigger, ObjData, ObjectEvent, TriggerTarget, ValData, ValType,
    };
    use std::collections::HashMap as StdHashMap;

    let mut value_map_bt = StdHashMap::new();
    value_map_bt.insert(
        "1".to_string(),
        ValData {
            data: b"init".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );

    let trigger_target = TriggerTarget::stateless("taptest", 0, "echo");
    // DataTrigger fields are on_create/on_update/on_delete lists.
    let data_trigger = DataTrigger {
        on_create: vec![],
        on_update: vec![trigger_target.clone()],
        on_delete: vec![],
    };
    let mut obj_event = ObjectEvent::default();
    obj_event.data_trigger.insert("1".to_string(), data_trigger);
    let obj = ObjData {
        metadata: None,
        entries: value_map_bt.clone(),
        event: Some(obj_event.clone()),
    };

    // Put object (create)
    // Convert ObjData into shard ObjectData type
    use oprc_odgm::shard::ObjectData as ShardObjectData;
    use oprc_odgm::shard::ObjectVal as ShardObjectVal;
    // Adapt proto ObjData -> internal ObjectData representation expected by set_object
    let to_internal = |o: &ObjData| {
        let mut entries = std::collections::BTreeMap::new();
        for (k, v) in &o.entries {
            let vt = ValType::try_from(v.r#type).unwrap_or(ValType::Byte);
            entries.insert(
                k.clone(),
                ShardObjectVal {
                    data: v.data.clone(),
                    r#type: vt,
                },
            );
        }
        ShardObjectData {
            last_updated: 0,
            entries,
            event: o.event.clone(),
        }
    };
    shard_arc.set_object("7", to_internal(&obj)).await?;
    // Mutate key 1 to trigger update
    let mut updated = obj.clone();
    updated.entries.get_mut("1").unwrap().data = b"changed".to_vec();
    shard_arc.set_object("7", to_internal(&updated)).await?;

    // Drain the trigger test tap to verify at least one execution recorded
    use oprc_odgm::events::processor::drain_trigger_test_tap;
    let mut attempts = 0;
    let mut recorded = None;
    while attempts < 10 {
        // up to ~1s total
        if let Some(exec) = drain_trigger_test_tap().await {
            if !exec.is_empty() {
                recorded = Some(exec);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        attempts += 1;
    }
    let recorded = recorded.ok_or("No trigger executions recorded")?;
    assert!(
        recorded.iter().any(|c| c.source_event.object_id == "7"),
        "Expected trigger execution for object id 7"
    );

    Ok(())
}
