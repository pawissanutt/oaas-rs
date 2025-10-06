use envconfig::Envconfig;
use oprc_dp_storage::AnyStorage;
use oprc_grpc::ValType;
use oprc_odgm::events::EventManagerImpl;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::replication::no_replication::NoReplication; // retained for type
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::UnifiedShardConfig;
use oprc_odgm::shard::unified::UnifiedShardFactory;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::unified::{ObjectUnifiedShard, ShardError};

type TestShard = ObjectUnifiedShard<
    AnyStorage,
    NoReplication<AnyStorage>,
    EventManagerImpl<AnyStorage>,
>;

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 7771,
        collection: "bridge".into(),
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
    // Use factory so bridge dispatcher is created.
    let pool = {
        let z_conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
        oprc_zenoh::pool::Pool::new(1, z_conf)
    };
    let factory_cfg = UnifiedShardConfig {
        enable_string_ids: true,
        max_string_id_len: 160,
        granular_prefetch_limit: 256,
    };
    let factory = UnifiedShardFactory::new(pool, factory_cfg);
    let shard = factory.create_basic_shard(metadata()).await?;
    Ok(shard)
}

#[ignore]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_bridge_summary_metrics_increment() -> Result<(), ShardError> {
    let shard = shard().await?;
    let id = "tenant::bridge";
    // Subscribe to events (optional validate at least one received)
    let mut maybe_rx = shard.bridge_subscribe();

    let before_emit = shard.bridge_events_emitted();

    shard.set_entry(id, "k1", val("v1")).await?;
    shard.set_entry(id, "k2", val("v2")).await?;
    shard
        .batch_set_entries(
            id,
            vec![("k3".to_string(), val("v3"))].into_iter().collect(),
            None,
        )
        .await?;

    // allow dispatcher task to process (bridge consumer logs asynchronously)
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let after_emit = shard.bridge_events_emitted();
    assert!(
        after_emit >= before_emit + 3,
        "expected at least 3 summary events (one per mutation), got {} -> {}",
        before_emit,
        after_emit
    );
    if let Some(rx) = maybe_rx.as_mut() {
        // Non-fatal assertion: ensure at least one broadcast received
        let mut received = 0u32;
        while let Ok(_evt) = rx.try_recv() {
            received += 1;
        }
        assert!(received >= 1, "expected at least one broadcast event");
    }
    Ok(())
}
