/// Integration test: verify that the V2 event pipeline publishes mutation events
/// to Zenoh when `ODGM_ZENOH_EVENT_PUBLISH=true`.
///
/// This bridges the gap between the v2_basic_test (in-process broadcast only) and the
/// system E2E test (full cluster). It exercises ODGM → Zenoh publish → Zenoh subscribe
/// in a single process without Gateway or a Kind cluster.
use oprc_grpc::ValType;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::shard::ObjectShard;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::traits::ShardMetadata;
use oprc_odgm::shard::{ShardBuilder, ShardOptions};
use oprc_zenoh::pool::Pool;
use oprc_zenoh::{Envconfig, OprcZenohConfig};
use std::time::Duration;

const CLS: &str = "ZenohPubTest";

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 7777,
        collection: CLS.into(),
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

/// Core test: write an entry → receive the event on Zenoh subscriber.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_zenoh_publish_on_set_entry() {
    // Enable V2 pipeline + Zenoh publication
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
        // Use SessionLocal so the event stays in-process (no router needed)
        std::env::set_var("ODGM_ZENOH_EVENT_LOCALITY", "session_local");
    }

    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("zenoh session");

    // Subscribe to the Zenoh topic BEFORE building the shard
    let topic = format!("oprc/{}/0/events/obj1", CLS);
    let subscriber = session
        .declare_subscriber(&topic)
        .await
        .expect("declare subscriber");

    let shard = ShardBuilder::new()
        .metadata(metadata())
        .session(session.clone())
        .options(ShardOptions::new(64, 64))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");

    // Write an entry — should trigger V2 event → Zenoh publish
    shard.set_entry("obj1", "key1", val("hello")).await.unwrap();

    // Receive event from Zenoh
    let sample = tokio::time::timeout(Duration::from_secs(5), subscriber.recv_async())
        .await
        .expect("Zenoh event timed out — V2 dispatcher did not publish to Zenoh")
        .expect("subscriber recv error");

    let payload: serde_json::Value =
        serde_json::from_slice(&sample.payload().to_bytes()).expect("JSON parse");

    assert_eq!(payload["object_id"], "obj1");
    assert_eq!(payload["cls_id"], CLS);
    assert_eq!(payload["partition_id"], 0);
    assert_eq!(payload["changes"][0]["key"], "key1");
    assert_eq!(payload["changes"][0]["action"], "create");
}

/// Verify that update and delete mutations also publish to Zenoh.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_zenoh_publish_create_update_delete() {
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_LOCALITY", "session_local");
    }

    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("zenoh session");

    let oid = "obj-cud";
    let topic = format!("oprc/{}/0/events/{}", CLS, oid);
    let subscriber = session
        .declare_subscriber(&topic)
        .await
        .expect("declare subscriber");

    let shard = ShardBuilder::new()
        .metadata(metadata())
        .session(session.clone())
        .options(ShardOptions::new(64, 64))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");

    // Create
    shard.set_entry(oid, "k", val("v1")).await.unwrap();
    // Update
    shard.set_entry(oid, "k", val("v2")).await.unwrap();
    // Delete
    shard.delete_entry(oid, "k").await.unwrap();

    // Collect 3 Zenoh events
    let mut actions = Vec::new();
    for _ in 0..3 {
        let sample =
            tokio::time::timeout(Duration::from_secs(5), subscriber.recv_async())
                .await
                .expect("Zenoh event timed out")
                .expect("recv error");
        let payload: serde_json::Value =
            serde_json::from_slice(&sample.payload().to_bytes()).unwrap();
        let action = payload["changes"][0]["action"]
            .as_str()
            .unwrap()
            .to_string();
        actions.push(action);
    }

    assert_eq!(actions, vec!["create", "update", "delete"]);
}

/// Verify that batch_set_entries also triggers Zenoh publication.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn v2_zenoh_publish_batch_set() {
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_LOCALITY", "session_local");
    }

    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("zenoh session");

    let oid = "obj-batch";
    let topic = format!("oprc/{}/0/events/{}", CLS, oid);
    let subscriber = session
        .declare_subscriber(&topic)
        .await
        .expect("declare subscriber");

    let shard = ShardBuilder::new()
        .metadata(metadata())
        .session(session.clone())
        .options(ShardOptions::new(64, 64))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build()
        .await
        .expect("shard");
    shard.initialize().await.expect("init");

    // Batch write multiple keys at once
    let mut entries = std::collections::HashMap::new();
    entries.insert("a".to_string(), val("1"));
    entries.insert("b".to_string(), val("2"));
    entries.insert("c".to_string(), val("3"));
    shard.batch_set_entries(oid, entries, None).await.unwrap();

    // Should receive exactly one Zenoh event with 3 changes
    let sample =
        tokio::time::timeout(Duration::from_secs(5), subscriber.recv_async())
            .await
            .expect("Zenoh event timed out on batch_set")
            .expect("recv error");
    let payload: serde_json::Value =
        serde_json::from_slice(&sample.payload().to_bytes()).unwrap();

    assert_eq!(payload["object_id"], oid);
    let changes = payload["changes"].as_array().expect("changes array");
    assert_eq!(changes.len(), 3, "batch should emit 3 changes");

    // All should be "create" since keys didn't exist before
    for ch in changes {
        assert_eq!(ch["action"], "create");
    }
}
