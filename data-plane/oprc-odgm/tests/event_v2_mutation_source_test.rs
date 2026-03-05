//! E2E test: verify MutationSource is correctly tagged on local writes,
//! and that events are published to Zenoh when zenoh_event_publish is enabled.

use oprc_grpc::ValType;
use oprc_odgm::events::MutationSource;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::shard::ObjectShard;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::traits::ShardMetadata;
use oprc_odgm::shard::{ShardBuilder, ShardOptions};
use oprc_zenoh::pool::Pool;
use oprc_zenoh::{Envconfig, OprcZenohConfig};

fn metadata(id: u64, cls: &str) -> ShardMetadata {
    ShardMetadata {
        id,
        collection: cls.into(),
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

/// Local writes through EntryStore should tag events with MutationSource::Local.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_local_mutation_source_on_set() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("session");
    let shard = ShardBuilder::new()
        .metadata(metadata(5001, "src_local"))
        .session(session)
        .options(ShardOptions::new(64, 64))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");

    let mut rx = shard.v2_subscribe().expect("v2 dispatcher present");

    // set_entry is a local operation
    shard.set_entry("obj1", "k1", val("hello")).await.unwrap();

    let evt =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("evt");
    assert_eq!(evt.ctx.object_id, "obj1");
    assert_eq!(
        evt.ctx.source,
        MutationSource::Local,
        "local writes must have source=Local"
    );
}

/// Delete operations should also tag with MutationSource::Local.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_local_mutation_source_on_delete() {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("session");
    let shard = ShardBuilder::new()
        .metadata(metadata(5002, "src_del"))
        .session(session)
        .options(ShardOptions::new(64, 64))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");

    let mut rx = shard.v2_subscribe().expect("v2 dispatcher present");

    // Create then delete
    shard.set_entry("obj2", "k1", val("v")).await.unwrap();
    let _create =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("create evt");

    shard.delete_entry("obj2", "k1").await.unwrap();
    let del_evt =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout")
            .expect("delete evt");
    assert_eq!(del_evt.ctx.object_id, "obj2");
    assert_eq!(
        del_evt.ctx.source,
        MutationSource::Local,
        "delete must have source=Local"
    );
}

/// When zenoh_event_publish is enabled, processed events should appear on
/// the Zenoh bus with SessionLocal scope.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_zenoh_session_local_publication() {
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
    };
    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("session");

    // Subscribe to the event topic on the SAME session (SessionLocal)
    let sub = session
        .declare_subscriber("oprc/zpub/0/events/**")
        .await
        .expect("declare sub");

    let shard = ShardBuilder::new()
        .metadata(metadata(5003, "zpub"))
        .session(session)
        .options(ShardOptions::new(64, 64))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");

    // Write an entry → should trigger V2 event → Zenoh publication
    shard.set_entry("abc", "x", val("y")).await.unwrap();

    // Receive the Zenoh message
    let sample = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        sub.recv_async(),
    )
    .await
    .expect("timeout waiting for Zenoh event")
    .expect("recv");

    // Verify it's JSON with the expected fields
    let payload = sample.payload().to_bytes();
    let json: serde_json::Value =
        serde_json::from_slice(&payload).expect("parse JSON");
    assert_eq!(json["object_id"], "abc");
    assert_eq!(json["cls_id"], "zpub");
    assert_eq!(json["source"], "local");
    assert!(json["changes"].is_array());
    assert_eq!(json["changes"][0]["key"], "x");

    // Clean up
    unsafe {
        std::env::remove_var("ODGM_ZENOH_EVENT_PUBLISH");
    };
}
