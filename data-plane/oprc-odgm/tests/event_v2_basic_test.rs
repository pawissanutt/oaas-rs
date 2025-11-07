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

fn val(data: &str) -> ObjectVal {
    ObjectVal {
        data: data.as_bytes().to_vec(),
        r#type: ValType::Byte,
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_basic_enqueue_and_receive() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_BRIDGE", "false") };
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let factory = UnifiedShardFactory::new(
        pool,
        UnifiedShardConfig {
            max_string_id_len: 64,
            granular_prefetch_limit: 64,
        },
    );
    let metadata = ShardMetadata {
        id: 1,
        collection: "cls".into(),
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
    };
    let shard = factory.create_basic_shard(metadata).await.expect("shard");
    shard.initialize().await.expect("init");

    let mut rx = shard.v2_subscribe().expect("v2 dispatcher present");
    shard.set_entry("1", "k1", val("v")).await.unwrap();

    let evt =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("evt");
    assert_eq!(evt.ctx.object_id, "1");
    assert_eq!(evt.ctx.changed.len(), 1);
    assert_eq!(evt.ctx.changed[0].key_canonical, "k1");
}
