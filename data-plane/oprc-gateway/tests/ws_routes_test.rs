//! E2E test: verify WebSocket routes are correctly registered in the Gateway
//! based on the `ws_enabled` configuration flag, and that WS message delivery works.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use envconfig::Envconfig;
use std::time::Duration;
use tower::util::ServiceExt;

/// When ws_enabled=false, WS routes should return 404 (catch-all).
#[tokio::test(flavor = "multi_thread")]
async fn ws_routes_return_404_when_disabled() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(50), false);

    // Object-level WS endpoint
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/0/objects/42/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "WS object route should be 404 when disabled"
    );

    // Partition-level WS endpoint
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/0/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "WS partition route should be 404 when disabled"
    );

    // Class-level WS endpoint
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NOT_FOUND,
        "WS class route should be 404 when disabled"
    );
}

/// When ws_enabled=true, WS routes exist and respond (not 404).
/// A non-upgrade GET request to a WS endpoint should return a method-related
/// error rather than 404, confirming the route IS registered.
#[tokio::test(flavor = "multi_thread")]
async fn ws_routes_registered_when_enabled() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(50), true);

    // Object-level WS endpoint — without upgrade headers, axum returns
    // an error status but NOT 404, proving the route exists.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/0/objects/42/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "WS object route should be registered when enabled (got {})",
        res.status()
    );

    // Partition-level WS endpoint
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/0/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "WS partition route should be registered when enabled (got {})",
        res.status()
    );

    // Class-level WS endpoint
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/ws")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        res.status(),
        StatusCode::NOT_FOUND,
        "WS class route should be registered when enabled (got {})",
        res.status()
    );
}

/// Test WS message delivery: connect a real WebSocket client,
/// publish an event on the same Zenoh session, and verify the client receives it.
#[tokio::test(flavor = "multi_thread")]
async fn ws_object_receives_zenoh_event() {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;

    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let z = session.clone();

    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200), true);

    // Bind to random port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn server
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect WS client
    let url = format!(
        "ws://{}/api/class/TestCls/0/objects/obj1/ws",
        addr
    );
    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async(&url).await.expect("WS connect");

    // Give subscriber time to be declared
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish a test event on the matching Zenoh topic
    let event_json = r#"{"object_id":"obj1","cls_id":"TestCls","partition_id":0,"changes":[{"key":"k1","action":"Create"}]}"#;
    z.put("oprc/TestCls/0/events/obj1", event_json)
        .await
        .expect("zenoh put");

    // Read from WS with timeout
    let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
        .await
        .expect("WS receive timeout")
        .expect("WS stream ended")
        .expect("WS message error");

    let text = match msg {
        tokio_tungstenite::tungstenite::Message::Text(t) => t,
        other => panic!("expected Text frame, got {:?}", other),
    };
    assert!(
        text.contains("obj1"),
        "WS message should contain object_id: {}",
        text
    );
    assert!(
        text.contains("Create"),
        "WS message should contain change action: {}",
        text
    );

    // Close WS
    ws_stream.close(None).await.ok();
}

/// Test class-level WS: subscribe to all objects in a class and verify events
/// from different partitions/objects are received.
#[tokio::test(flavor = "multi_thread")]
async fn ws_class_receives_events_from_multiple_partitions() {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;

    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let z = session.clone();

    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200), true);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect class-level WS
    let url = format!("ws://{}/api/class/MyCls/ws", addr);
    let (mut ws_stream, _) =
        tokio_tungstenite::connect_async(&url).await.expect("WS connect");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Publish events on two different partitions
    let ev1 = r#"{"object_id":"a","cls_id":"MyCls","partition_id":0,"changes":[{"key":"x","action":"Create"}]}"#;
    let ev2 = r#"{"object_id":"b","cls_id":"MyCls","partition_id":1,"changes":[{"key":"y","action":"Update"}]}"#;

    z.put("oprc/MyCls/0/events/a", ev1)
        .await
        .expect("put ev1");
    z.put("oprc/MyCls/1/events/b", ev2)
        .await
        .expect("put ev2");

    // Collect 2 messages
    let mut received = Vec::new();
    for _ in 0..2 {
        let msg =
            tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
                .await
                .expect("WS receive timeout")
                .expect("WS stream ended")
                .expect("WS message error");
        if let tokio_tungstenite::tungstenite::Message::Text(t) = msg {
            received.push(t.to_string());
        }
    }

    assert_eq!(received.len(), 2, "should receive 2 events");
    let combined = received.join(" ");
    assert!(combined.contains("\"a\""), "should contain object a");
    assert!(combined.contains("\"b\""), "should contain object b");

    ws_stream.close(None).await.ok();
}
