use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bytes::Bytes;
use envconfig::Envconfig;
use http_body_util::Full;
use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjData, ObjMeta,
    ObjectInvocationRequest, ResponseStatus,
};
use oprc_zenoh::util::{Handler, ManagedConfig, declare_managed_queryable};
use prost::Message;
use std::time::Duration;
use tower::util::ServiceExt;
use zenoh::query::Query;

// Minimal smoke test: build router with a zenoh session mocked by opening in peerless mode.
#[tokio::test(flavor = "multi_thread")]
async fn health_endpoints_work() {
    // Create a zenoh session in peer mode without peers; this should open locally.
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(50));

    // healthz
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // readyz
    let res = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[derive(Clone)]
struct GatewayTestHandler;

#[async_trait::async_trait]
impl Handler<Query> for GatewayTestHandler {
    async fn handle(&self, query: Query) {
        let key = query.key_expr().as_str();
        if key.contains("/invokes/") {
            // Invocation path
            let payload = query.payload().expect("invoke requires payload");
            if key.contains("/objects/") {
                let req: ObjectInvocationRequest =
                    oprc_invoke::serde::decode(payload).expect("decode");
                let mut headers = std::collections::HashMap::new();
                let ct =
                    req.options.get("accept").cloned().unwrap_or_else(|| {
                        "application/octet-stream".to_string()
                    });
                headers.insert("content-type".to_string(), ct);
                let resp = InvocationResponse {
                    payload: Some(req.payload),
                    status: ResponseStatus::Okay.into(),
                    headers,
                    invocation_id: "t1".into(),
                };
                let _ =
                    query.reply(query.key_expr(), resp.encode_to_vec()).await;
            } else {
                let req: InvocationRequest =
                    oprc_invoke::serde::decode(payload).expect("decode");
                let mut headers = std::collections::HashMap::new();
                let ct =
                    req.options.get("accept").cloned().unwrap_or_else(|| {
                        "application/octet-stream".to_string()
                    });
                headers.insert("content-type".to_string(), ct);
                let resp = InvocationResponse {
                    payload: Some(req.payload),
                    status: ResponseStatus::Okay.into(),
                    headers,
                    invocation_id: "t2".into(),
                };
                let _ =
                    query.reply(query.key_expr(), resp.encode_to_vec()).await;
            }
        } else if key.ends_with("/set") {
            let _ = query.reply(query.key_expr(), b"ok".to_vec()).await;
        } else if key.ends_with("/objects/404") {
            // simulate not found by returning an ObjData without metadata
            let obj = ObjData {
                metadata: None,
                entries: Default::default(),
                entries_str: Default::default(),
                event: None,
            };
            let _ = query.reply(query.key_expr(), obj.encode_to_vec()).await;
        } else if key.contains("/objects/") {
            // GET object
            let parts: Vec<&str> = key.split('/').collect();
            let cls = parts[1].to_string();
            let pid: u32 = parts[2].parse().unwrap_or(0);
            let (oid, oid_str) = match parts[4].parse::<u64>() {
                Ok(num) => (num, None),
                Err(_) => (0u64, Some(parts[4].to_string())),
            };
            let obj = ObjData {
                metadata: Some(ObjMeta {
                    cls_id: cls,
                    partition_id: pid,
                    object_id: oid,
                    object_id_str: oid_str,
                }),
                entries: Default::default(),
                entries_str: Default::default(),
                event: None,
            };
            let _ = query.reply(query.key_expr(), obj.encode_to_vec()).await;
        } else {
            // default: not matched
            let _ = query.reply_err(b"no handler".to_vec()).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_object_content_negotiation() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let _q = declare_managed_queryable(
        &session,
        ManagedConfig::unbounded("oprc/**", 2),
        GatewayTestHandler,
    )
    .await
    .expect("declare q");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));

    // Default protobuf
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/1/objects/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res
        .headers()
        .get(http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/x-protobuf");

    // JSON via Accept
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/1/objects/42")
                .header(http::header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res
        .headers()
        .get(http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/json");
}

#[tokio::test(flavor = "multi_thread")]
async fn put_object_roundtrip() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let _q = declare_managed_queryable(
        &session,
        ManagedConfig::unbounded("oprc/**", 2),
        GatewayTestHandler,
    )
    .await
    .expect("declare q");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));

    let meta = ObjMeta {
        cls_id: "Counter".into(),
        partition_id: 1,
        object_id: 42,
        object_id_str: None,
    };
    let obj = ObjData {
        metadata: Some(meta),
        entries: Default::default(),
        entries_str: Default::default(),
        event: None,
    };
    let body = Body::new(Full::from(Bytes::from(obj.encode_to_vec())));
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/class/Counter/1/objects/42")
                .header(http::header::CONTENT_TYPE, "application/x-protobuf")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn invoke_fn_honors_accept_json() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let _q = declare_managed_queryable(
        &session,
        ManagedConfig::unbounded("oprc/**", 2),
        GatewayTestHandler,
    )
    .await
    .expect("declare q");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/class/Foo/0/invokes/echo")
                .header(http::header::ACCEPT, "application/json")
                .header(http::header::CONTENT_TYPE, "application/octet-stream")
                .body(Body::new(Full::from(Bytes::from_static(b"ping"))))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res
        .headers()
        .get(http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/json");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_object_with_string_id() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let _q = declare_managed_queryable(
        &session,
        ManagedConfig::unbounded("oprc/**", 2),
        GatewayTestHandler,
    )
    .await
    .expect("declare q");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/1/objects/alpha-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_object_invalid_char_rejected() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/1/objects/Bad$Id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_object_over_length_rejected() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));
    let long_id = "a".repeat(161);
    let uri = format!("/api/class/Counter/1/objects/{}", long_id);
    let res = app
        .oneshot(
            Request::builder()
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_idempotent_and_404_error_body() {
    let cfg = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let session = zenoh::open(cfg.create_zenoh()).await.expect("zenoh open");
    let _q = declare_managed_queryable(
        &session,
        ManagedConfig::unbounded("oprc/**", 2),
        GatewayTestHandler,
    )
    .await
    .expect("declare q");
    let app: Router =
        oprc_gateway::build_router(session, Duration::from_millis(200));

    // First DELETE returns 204
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/class/Counter/1/objects/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    // Second DELETE should also return 204 (idempotent)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/class/Counter/1/objects/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // GET nonexistent id -> 404 with JSON error body
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/class/Counter/1/objects/404")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let ct = res
        .headers()
        .get(http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "application/json");
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let s = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(s.contains("\"error\""));
    assert!(s.contains("NO_OBJECT") || s.contains("NOT_FOUND"));
}
