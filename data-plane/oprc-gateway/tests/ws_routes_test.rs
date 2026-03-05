//! E2E test: verify WebSocket routes are correctly registered in the Gateway
//! based on the `ws_enabled` configuration flag.

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

    // Both routes should exist; verify they don't collide with REST routes
}
