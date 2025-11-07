use oprc_dp_storage::StorageConfig;
use oprc_dp_storage::backends::memory::MemoryStorage;
use oprc_grpc::ValType;
use oprc_odgm::events::EventManagerImpl;
use oprc_odgm::granular_key::{
    GranularRecord, build_metadata_key, parse_granular_key,
};
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::UnifiedShardConfig;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::unified::{ObjectUnifiedShard, ShardError};

type TestShard = ObjectUnifiedShard<
    MemoryStorage,
    NoReplication<MemoryStorage>,
    EventManagerImpl<MemoryStorage>,
>;

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 901,
        collection: "phase-h".into(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "memory".into(),
        options: Default::default(),
        invocations: Default::default(),
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}

fn val(data: &str) -> ObjectVal {
    ObjectVal {
        data: data.as_bytes().to_vec(),
        r#type: ValType::Byte,
    }
}

async fn shard() -> Result<TestShard, ShardError> {
    let storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
    let replication = NoReplication::new(storage.clone());
    let cfg = UnifiedShardConfig {
        max_string_id_len: 160,
        granular_prefetch_limit: 256,
    };
    let s =
        ObjectUnifiedShard::new_minimal(metadata(), storage, replication, cfg)
            .await?;
    s.initialize().await?;
    Ok(s)
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_no_blob_key_after_mutations() -> Result<(), ShardError> {
    let shard = shard().await?;
    let object_id = "tenant::noblob".to_string();

    // Perform mixed mutations
    shard.set_entry(&object_id, "a", val("1")).await?;
    shard.set_entry(&object_id, "b", val("2")).await?;
    shard.set_entry(&object_id, "a", val("3")).await?; // update
    shard.delete_entry(&object_id, "b").await?; // delete one
    shard.set_entry(&object_id, "c", val("4")).await?;

    // Retrieve raw keys (metadata + entries) via debug helper
    let raw_keys = shard.debug_raw_keys_for_object(&object_id).await?;
    assert!(raw_keys.len() >= 1, "should at least have metadata key");

    // Collect classification; ensure only metadata + entry records exist.
    let mut saw_meta = false;
    for k in &raw_keys {
        match parse_granular_key(k.as_slice()) {
            Some((ref id, GranularRecord::Metadata)) => {
                assert_eq!(id, &object_id);
                saw_meta = true;
            }
            Some((ref id, GranularRecord::Entry(_))) => {
                assert_eq!(id, &object_id)
            }
            other => panic!("unexpected non-granular key present: {:?}", other),
        }
    }
    assert!(saw_meta, "metadata key must exist");
    let expected_meta = build_metadata_key(&object_id);
    let meta_count = raw_keys.iter().filter(|k| **k == expected_meta).count();
    assert_eq!(meta_count, 1, "exactly one metadata key expected");

    Ok(())
}
