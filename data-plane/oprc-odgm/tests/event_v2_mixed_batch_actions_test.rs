use oprc_grpc::ValType;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::shard::ObjectShard;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::traits::ShardMetadata;
use oprc_odgm::shard::{ShardBuilder, ShardOptions};
use oprc_zenoh::pool::Pool;
use oprc_zenoh::{Envconfig, OprcZenohConfig};
use std::collections::HashMap;

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 9202,
        collection: "v2mix".into(),
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
async fn v2_mixed_batch_actions() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("session");
    let shard = ShardBuilder::new()
        .metadata(metadata())
        .session(session)
        .options(ShardOptions::new(128, 128))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");
    let mut rx = shard.v2_subscribe().expect("dispatcher");
    let oid = "mix::1";
    shard.set_entry(oid, "existing", val("v1")).await.unwrap(); // first event create
    let evt1 =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(evt1.ctx.changed[0].key_canonical, "existing");
    // Batch with existing (update) and new (create)
    let mut batch = HashMap::new();
    batch.insert("existing".to_string(), val("v2"));
    batch.insert("new_k".to_string(), val("v3"));
    shard.batch_set_entries(oid, batch, None).await.unwrap();
    let evt2 =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
    use oprc_odgm::events::MutAction;
    let mut found_update = false;
    let mut found_create = false;
    for ck in evt2.ctx.changed {
        match ck.action {
            MutAction::Update if ck.key_canonical == "existing" => {
                found_update = true
            }
            MutAction::Create if ck.key_canonical == "new_k" => {
                found_create = true
            }
            _ => {}
        }
    }
    assert!(
        found_update && found_create,
        "expected update and create actions in mixed batch"
    );
}
