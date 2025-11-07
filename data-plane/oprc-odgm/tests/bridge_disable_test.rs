use envconfig::Envconfig;
use oprc_dp_storage::AnyStorage;
use oprc_grpc::ValType;
use oprc_odgm::events::EventManagerImpl;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::UnifiedShardConfig;
use oprc_odgm::shard::unified::UnifiedShardFactory;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::unified::{ObjectUnifiedShard, ShardError};

// Alias matching factory generics
type TestShard = ObjectUnifiedShard<
    AnyStorage,
    NoReplication<AnyStorage>,
    EventManagerImpl<AnyStorage>,
>;

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 7781,
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

async fn make_shard() -> Result<TestShard, ShardError> {
    let z_conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let pool = oprc_zenoh::pool::Pool::new(1, z_conf);
    let factory_cfg = UnifiedShardConfig {
        max_string_id_len: 160,
        granular_prefetch_limit: 256,
    };
    let factory = UnifiedShardFactory::new(pool, factory_cfg);
    let shard = factory.create_basic_shard(metadata()).await?;
    Ok(shard)
}

#[ignore]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_bridge_disabled_by_v2_override() -> Result<(), ShardError> {
    // Ensure override variable disables bridge (set before shard creation)
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::remove_var("ODGM_EVENT_PIPELINE_BRIDGE");
    }
    let shard = make_shard().await?;
    let id = "tenant::bridge";
    let before_emit = shard.bridge_events_emitted();
    shard.set_entry(id, "a", val("1")).await?;
    shard.set_entry(id, "b", val("2")).await?;
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let after_emit = shard.bridge_events_emitted();
    assert_eq!(
        before_emit, after_emit,
        "bridge events should remain unchanged when V2 override is set"
    );
    Ok(())
}
