use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::shard::unified::factory::{
    UnifiedShardConfig, UnifiedShardFactory,
};
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_zenoh::pool::Pool;
use oprc_zenoh::{Envconfig, OprcZenohConfig};

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 9303,
        collection: "v2empty".into(),
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

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_empty_batch_noop() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_BRIDGE", "false") };
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let factory = UnifiedShardFactory::new(
        pool,
        UnifiedShardConfig {
            max_string_id_len: 128,
            granular_prefetch_limit: 128,
        },
    );
    let shard = factory.create_basic_shard(metadata()).await.expect("shard");
    shard.initialize().await.expect("init");
    let oid = "empty::1";
    // Get baseline version
    let before = shard
        .get_metadata(oid)
        .await
        .unwrap_or_default()
        .map(|m| m.object_version)
        .unwrap_or(0);
    let ver_returned = shard
        .batch_set_entries(oid, std::collections::HashMap::new(), None)
        .await
        .unwrap();
    let after_meta = shard
        .get_metadata(oid)
        .await
        .unwrap_or_default()
        .map(|m| m.object_version)
        .unwrap_or(0);
    assert_eq!(
        before, after_meta,
        "version should not change on empty batch"
    );
    assert_eq!(
        before, ver_returned,
        "returned version should equal current version"
    );
}
