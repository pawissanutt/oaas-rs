use std::collections::HashMap;

use oprc_dp_storage::backends::fjall::FjallStorage;
use oprc_dp_storage::backends::memory::MemoryStorage;
use oprc_dp_storage::{ApplicationDataStorage, StorageConfig};
use oprc_grpc::ValType;
use oprc_odgm::events::EventManagerImpl;
use oprc_odgm::granular_key::build_entry_key;
use oprc_odgm::granular_trait::{EntryListOptions, EntryStore};
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::unified::{ObjectUnifiedShard, ShardError};
use tempfile::tempdir;

const ENABLE_STRING_IDS: bool = true;
const MAX_STRING_ID_LEN: usize = 160;

/// Convenient alias for the shard type used in granular storage tests.
type TestShard = ObjectUnifiedShard<
    MemoryStorage,
    NoReplication<MemoryStorage>,
    EventManagerImpl<MemoryStorage>,
>;
type FjallTestShard = ObjectUnifiedShard<
    FjallStorage,
    NoReplication<FjallStorage>,
    EventManagerImpl<FjallStorage>,
>;

fn create_test_metadata() -> ShardMetadata {
    ShardMetadata {
        id: 7,
        collection: "granular_tests".to_string(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "memory".to_string(),
        options: Default::default(),
        invocations: Default::default(),
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}

fn object_val(data: &str) -> ObjectVal {
    ObjectVal {
        data: data.as_bytes().to_vec(),
        r#type: ValType::Byte,
    }
}

async fn setup_shard() -> Result<(TestShard, String), ShardError> {
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let shard: TestShard = setup_shard_with_storage(storage).await?;
    Ok((shard, "tenant::object-1".to_string()))
}

async fn setup_shard_with_storage<S>(
    storage: S,
) -> Result<
    ObjectUnifiedShard<S, NoReplication<S>, EventManagerImpl<S>>,
    ShardError,
>
where
    S: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    let metadata = create_test_metadata();
    let replication = NoReplication::new(storage.clone());
    let shard = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        ENABLE_STRING_IDS,
        MAX_STRING_ID_LEN,
    )
    .await?;
    shard.initialize().await?;
    Ok(shard)
}

async fn collect_entries_paginated<T: EntryStore>(
    shard: &T,
    object_id: &str,
    mut options: EntryListOptions,
) -> Result<Vec<(String, ObjectVal)>, ShardError> {
    let mut cursor = options.cursor.take();
    let mut results = Vec::new();

    loop {
        let page_options = EntryListOptions {
            key_prefix: options.key_prefix.clone(),
            limit: options.limit,
            cursor: cursor.clone(),
        };

        let page = shard.list_entries(object_id, page_options).await?;
        results.extend(page.entries.iter().cloned());

        if let Some(next) = page.next_cursor {
            cursor = Some(next);
        } else {
            break;
        }
    }

    Ok(results)
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_entry_crud_and_versioning() -> Result<(), ShardError> {
    let (shard, object_id) = setup_shard().await?;

    assert!(
        shard.get_metadata(&object_id).await?.is_none(),
        "metadata should be absent before the first write"
    );

    let first = object_val("alpha-value");
    shard.set_entry(&object_id, "alpha", first.clone()).await?;

    let stored = shard
        .get_entry(&object_id, "alpha")
        .await?
        .expect("entry to be present after set");
    assert_eq!(stored, first);

    let metadata = shard
        .get_metadata(&object_id)
        .await?
        .expect("metadata to exist after first write");
    assert_eq!(metadata.object_version, 1, "first write increments version");

    let updated = object_val("alpha-updated");
    shard
        .set_entry(&object_id, "alpha", updated.clone())
        .await?;

    let metadata = shard
        .get_metadata(&object_id)
        .await?
        .expect("metadata to exist after update");
    assert_eq!(
        metadata.object_version, 2,
        "second write increments version"
    );

    shard.delete_entry(&object_id, "alpha").await?;
    let metadata = shard
        .get_metadata(&object_id)
        .await?
        .expect("metadata to exist after delete");
    assert_eq!(
        metadata.object_version, 3,
        "delete increments version when entry existed"
    );

    assert!(
        shard.get_entry(&object_id, "alpha").await?.is_none(),
        "entry should be deleted"
    );

    shard.delete_entry(&object_id, "alpha").await?;
    let metadata = shard
        .get_metadata(&object_id)
        .await?
        .expect("metadata to exist after idempotent delete");
    assert_eq!(
        metadata.object_version, 3,
        "idempotent delete should not bump version"
    );

    assert_eq!(
        shard.count_entries(&object_id).await?,
        0,
        "all entries should be removed"
    );

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_list_entries_prefix_and_ordering() -> Result<(), ShardError> {
    let (shard, object_id) = setup_shard().await?;

    let entries = vec![
        ("config:feature", "enabled"),
        ("user:100", "alice"),
        ("user:200", "bob"),
        ("user:210", "carol"),
        ("zeta", "tail"),
    ];

    for (key, value) in &entries {
        shard.set_entry(&object_id, key, object_val(value)).await?;
    }

    let listed = shard
        .list_entries(&object_id, EntryListOptions::default())
        .await?;
    let listed_keys: Vec<String> =
        listed.entries.iter().map(|(k, _)| k.clone()).collect();

    let mut expected_keys: Vec<(Vec<u8>, String)> = entries
        .iter()
        .map(|(k, _)| (build_entry_key(&object_id, k), k.to_string()))
        .collect();
    expected_keys.sort_by(|a, b| a.0.cmp(&b.0));
    let expected_keys: Vec<String> =
        expected_keys.into_iter().map(|(_, key)| key).collect();
    assert_eq!(
        listed_keys, expected_keys,
        "entries should follow storage ordering (length + lexicographic)"
    );

    let user_entries = shard
        .list_entries(
            &object_id,
            EntryListOptions::default().with_prefix("user:"),
        )
        .await?;
    let user_keys: Vec<String> = user_entries
        .entries
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        user_keys,
        vec!["user:100", "user:200", "user:210"],
        "prefix filter should match user entries"
    );

    assert_eq!(
        shard.count_entries(&object_id).await?,
        entries.len(),
        "count_entries should match number of stored entries"
    );

    // The stable ordering enables straightforward client-side pagination today.
    let chunk_sizes: Vec<usize> =
        listed_keys.chunks(2).map(|chunk| chunk.len()).collect();
    assert_eq!(
        chunk_sizes,
        vec![2, 2, 1],
        "chunking the ordered results provides deterministic pagination slices"
    );

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_batch_set_entries_and_version_cas() -> Result<(), ShardError> {
    let (shard, object_id) = setup_shard().await?;

    let initial_batch: HashMap<String, ObjectVal> = HashMap::from([
        ("alpha".to_string(), object_val("a")),
        ("beta".to_string(), object_val("b")),
    ]);

    let version = shard
        .batch_set_entries(&object_id, initial_batch, None)
        .await?;
    assert_eq!(version, 1, "first batch should return version 1");

    let metadata = shard
        .get_metadata(&object_id)
        .await?
        .expect("metadata to exist after batch set");
    assert_eq!(metadata.object_version, 1);

    let next_batch: HashMap<String, ObjectVal> = HashMap::from([
        ("alpha".to_string(), object_val("a2")),
        ("gamma".to_string(), object_val("c")),
    ]);
    let version = shard
        .batch_set_entries(&object_id, next_batch, Some(1))
        .await?;
    assert_eq!(
        version, 2,
        "second batch increments to version 2 when CAS matches"
    );

    let conflict = shard
        .batch_set_entries(&object_id, HashMap::new(), Some(1))
        .await;
    match conflict {
        Err(ShardError::VersionMismatch { expected, actual }) => {
            assert_eq!(expected, 1);
            assert_eq!(actual, 2);
        }
        other => panic!("expected version mismatch error, got {:?}", other),
    }

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_list_entries_pagination_cursor() -> Result<(), ShardError> {
    let (shard, object_id) = setup_shard().await?;

    for idx in 0..5 {
        let key = format!("key-{idx:03}");
        let value = format!("value-{idx}");
        shard
            .set_entry(&object_id, &key, object_val(&value))
            .await?;
    }

    let first_page = shard
        .list_entries(&object_id, EntryListOptions::with_limit(2))
        .await?;
    assert_eq!(first_page.entries.len(), 2);
    assert!(first_page.next_cursor.is_some());

    let second_options = EntryListOptions::with_limit(2)
        .with_cursor(first_page.next_cursor.clone().unwrap());
    let second_page = shard.list_entries(&object_id, second_options).await?;
    assert_eq!(second_page.entries.len(), 2);
    assert_ne!(first_page.entries[0].0, second_page.entries[0].0);

    let mut third_options = EntryListOptions::with_limit(2);
    if let Some(cursor) = second_page.next_cursor.clone() {
        third_options = third_options.with_cursor(cursor);
        let third_page = shard.list_entries(&object_id, third_options).await?;
        assert_eq!(third_page.entries.len(), 1);
        assert!(third_page.next_cursor.is_none());
    } else {
        panic!("expected third page cursor");
    }

    let collected = collect_entries_paginated(
        &shard,
        &object_id,
        EntryListOptions::with_limit(2),
    )
    .await?;
    assert_eq!(collected.len(), 5);

    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_list_entries_backend_parity() -> Result<(), ShardError> {
    let temp = tempdir().unwrap();
    let fjall_path = temp.path().join("fjall-granular-tests");
    let fjall_storage = FjallStorage::new(StorageConfig::fjall(
        fjall_path.to_string_lossy().to_string(),
    ))
    .unwrap();
    let memory_storage = MemoryStorage::new(StorageConfig::memory()).unwrap();

    let memory_shard: TestShard =
        setup_shard_with_storage(memory_storage).await?;
    let fjall_shard: FjallTestShard =
        setup_shard_with_storage(fjall_storage).await?;

    let object_id = "tenant::backend-parity";
    let entries = vec![
        ("config:feature", "enabled"),
        ("config:mode", "fast"),
        ("metrics", "on"),
        ("user:100", "alice"),
        ("user:200", "bob"),
        ("user:210", "carol"),
        ("user:220", "dave"),
        ("user:300", "eve"),
        ("zeta", "tail"),
    ];

    for (key, value) in &entries {
        memory_shard
            .set_entry(object_id, key, object_val(value))
            .await?;
        fjall_shard
            .set_entry(object_id, key, object_val(value))
            .await?;
    }

    let baseline_options = EntryListOptions::with_limit(4);
    let memory_page = memory_shard
        .list_entries(object_id, baseline_options.clone())
        .await?;
    let fjall_page = fjall_shard
        .list_entries(object_id, baseline_options)
        .await?;
    assert_eq!(memory_page.entries, fjall_page.entries);

    let prefix_options = EntryListOptions::with_limit(3).with_prefix("user:");
    let memory_prefix = memory_shard
        .list_entries(object_id, prefix_options.clone())
        .await?;
    let fjall_prefix =
        fjall_shard.list_entries(object_id, prefix_options).await?;
    assert_eq!(memory_prefix.entries, fjall_prefix.entries);

    let memory_all = collect_entries_paginated(
        &memory_shard,
        object_id,
        EntryListOptions::with_limit(5),
    )
    .await?;
    let fjall_all = collect_entries_paginated(
        &fjall_shard,
        object_id,
        EntryListOptions::with_limit(5),
    )
    .await?;

    assert_eq!(memory_all, fjall_all);

    Ok(())
}
