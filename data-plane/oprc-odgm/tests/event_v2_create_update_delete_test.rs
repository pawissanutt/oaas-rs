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
        id: 9101,
        collection: "v2act".into(),
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
async fn v2_create_update_delete_actions() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
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
    let mut rx = shard.v2_subscribe().expect("dispatcher");
    let oid = "obj::1";
    shard.set_entry(oid, "k", val("v1")).await.unwrap(); // create
    shard.set_entry(oid, "k", val("v2")).await.unwrap(); // update
    shard.delete_entry(oid, "k").await.unwrap(); // delete

    // Collect three events
    use oprc_odgm::events::MutAction;
    let mut actions = Vec::new();
    for _ in 0..3 {
        let evt =
            tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("timeout")
                .expect("evt");
        actions.push(evt.ctx.changed[0].action);
    }
    assert_eq!(
        actions,
        vec![MutAction::Create, MutAction::Update, MutAction::Delete]
    );

    // Idempotent delete (should not produce another event)
    shard.delete_entry(oid, "k").await.unwrap();
    let maybe_extra =
        tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await;
    assert!(
        maybe_extra.is_err(),
        "idempotent delete produced unexpected event"
    );
}
