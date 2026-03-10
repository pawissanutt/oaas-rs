/// Integration test: ODGM → Zenoh → Gateway WebSocket.
///
/// Spins up a real ODGM shard and Gateway in-process (same Zenoh session),
/// connects a WebSocket client to the Gateway, writes an entry to the shard,
/// and verifies that the WS client receives the event.
///
/// This validates the full data path without needing a Kind cluster:
///   shard.set_entry() → V2Dispatcher → Zenoh put → Gateway WS subscriber → WS client
use futures_util::StreamExt;
use oprc_grpc::ValType;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::shard::ObjectShard;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::traits::ShardMetadata;
use oprc_odgm::shard::{ShardBuilder, ShardOptions};
use oprc_zenoh::pool::Pool;
use oprc_zenoh::{Envconfig, OprcZenohConfig};
use std::time::Duration;

const CLS: &str = "GwIntTest";

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 8888,
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

/// End-to-end: ODGM write → Zenoh event → Gateway WS → client receives event.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn odgm_to_gateway_ws_object_event() {
    // Enable V2 pipeline + Zenoh publication with session_local locality
    // so events travel within this process without a Zenoh router.
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_LOCALITY", "session_local");
    }

    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("zenoh session");

    // ── Build ODGM shard ──
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
    shard.initialize().await.expect("shard init");

    // ── Build Gateway with WS enabled, sharing the same Zenoh session ──
    let app = oprc_gateway::build_router(session.clone(), Duration::from_millis(200), true);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ── Connect WS client ──
    let oid = "ws-obj-1";
    let url = format!(
        "ws://{}/api/class/{}/0/objects/{}/ws",
        addr, CLS, oid
    );
    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async(&url).await.expect("WS connect");

    // Wait for the Zenoh subscriber in the Gateway to be registered
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── Write to ODGM — this should trigger event → Zenoh → Gateway → WS ──
    shard.set_entry(oid, "color", val("#ff0000")).await.unwrap();

    // ── Receive from WS ──
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .expect("WS receive timed out — event didn't reach Gateway WS")
        .expect("WS stream ended")
        .expect("WS message error");

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
        other => panic!("expected Text frame, got {:?}", other),
    };

    let payload: serde_json::Value = serde_json::from_str(&text).expect("JSON parse");
    assert_eq!(payload["object_id"], oid);
    assert_eq!(payload["cls_id"], CLS);
    assert_eq!(payload["partition_id"], 0);
    assert_eq!(payload["changes"][0]["key"], "color");
    assert_eq!(payload["changes"][0]["action"], "create");

    // Clean close
    ws_stream.close(None).await.ok();
}

/// Class-level WS: subscribe to all events in a class, write to two different objects,
/// verify both appear on the WS.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn odgm_to_gateway_ws_class_level() {
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_LOCALITY", "session_local");
    }

    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("zenoh session");

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
    shard.initialize().await.expect("shard init");

    let app = oprc_gateway::build_router(session.clone(), Duration::from_millis(200), true);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Class-level WS subscription
    let url = format!("ws://{}/api/class/{}/ws", addr, CLS);
    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async(&url).await.expect("WS connect");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Write to two different objects
    shard.set_entry("alpha", "x", val("1")).await.unwrap();
    shard.set_entry("beta", "y", val("2")).await.unwrap();

    // Collect 2 events
    let mut received = Vec::new();
    for _ in 0..2 {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .expect("WS timeout")
            .expect("WS stream ended")
            .expect("WS error");
        if let tokio_tungstenite::tungstenite::Message::Text(t) = msg {
            received.push(t.to_string());
        }
    }

    assert_eq!(received.len(), 2, "should receive 2 events");
    let combined = received.join(" ");
    assert!(combined.contains("alpha"), "should see object alpha");
    assert!(combined.contains("beta"), "should see object beta");

    ws_stream.close(None).await.ok();
}

/// Update + delete: verify that non-create mutations also flow through WS.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn odgm_to_gateway_ws_update_and_delete() {
    unsafe {
        std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_PUBLISH", "true");
        std::env::set_var("ODGM_ZENOH_EVENT_LOCALITY", "session_local");
    }

    let cfg = OprcZenohConfig::init_from_env().unwrap();
    let pool = Pool::new(1, cfg);
    let session = pool.get_session().await.expect("zenoh session");

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
    shard.initialize().await.expect("shard init");

    let app = oprc_gateway::build_router(session.clone(), Duration::from_millis(200), true);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    let oid = "mut-obj";
    let url = format!("ws://{}/api/class/{}/0/objects/{}/ws", addr, CLS, oid);
    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async(&url).await.expect("WS connect");

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create → Update → Delete
    shard.set_entry(oid, "k", val("v1")).await.unwrap();
    shard.set_entry(oid, "k", val("v2")).await.unwrap();
    shard.delete_entry(oid, "k").await.unwrap();

    let mut actions = Vec::new();
    for _ in 0..3 {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .expect("WS timeout")
            .expect("WS stream ended")
            .expect("WS error");
        if let tokio_tungstenite::tungstenite::Message::Text(t) = msg {
            let payload: serde_json::Value = serde_json::from_str(&t).unwrap();
            let action = payload["changes"][0]["action"]
                .as_str()
                .unwrap()
                .to_string();
            actions.push(action);
        }
    }

    assert_eq!(actions, vec!["create", "update", "delete"]);

    ws_stream.close(None).await.ok();
}
