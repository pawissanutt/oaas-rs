use oprc_grpc::ValType;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::unified::ObjectShard;
use oprc_odgm::shard::unified::factory::{
    UnifiedShardConfig, UnifiedShardFactory,
};
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_zenoh::pool::Pool;
use oprc_zenoh::{Envconfig, OprcZenohConfig};

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 9303,
        collection: "v2sum".into(),
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
fn val(d: &str) -> ObjectVal {
    ObjectVal {
        data: d.as_bytes().to_vec(),
        r#type: ValType::Byte,
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_fanout_summary_when_over_cap() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_BRIDGE", "false") };
    unsafe { std::env::set_var("ODGM_MAX_BATCH_TRIGGER_FANOUT", "1") }; // cap at 1 to force summary
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let factory = UnifiedShardFactory::new(
        pool,
        UnifiedShardConfig {
            max_string_id_len: 64,
            granular_prefetch_limit: 64,
        },
    );
    let shard = factory.create_basic_shard(metadata()).await.expect("shard");
    shard.initialize().await.expect("init");

    let mut rx = shard.v2_subscribe().expect("v2 dispatcher present");

    // create object with two mutations in one batch
    let oid = "obj::cap";
    shard.set_entry(oid, "k1", val("v1")).await.unwrap();
    // batch with two keys to exceed cap
    let mut batch = std::collections::HashMap::new();
    batch.insert("k2".to_string(), val("v2"));
    batch.insert("k3".to_string(), val("v3"));
    shard.batch_set_entries(oid, batch, None).await.unwrap();

    // drain first event (create for k1)
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    // second event should contain summary due to fanout cap on the batch
    let evt =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
    let summary = evt.summary.expect("expected summary on truncated batch");
    assert_eq!(
        summary.emitted, 1,
        "only one entry should be emitted under cap"
    );
    assert!(
        summary.changed_total >= 2,
        "batch should have >= 2 changed keys"
    );
    assert!(
        !summary.sample_remaining_keys.is_empty(),
        "sample remaining keys should be present"
    );
}
