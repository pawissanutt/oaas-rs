//! Integration tests for Phase 6 (Artifact Storage) and Phase 7 (Script Endpoints).
//!
//! These tests exercise the full HTTP API stack: handlers → services → storage,
//! using in-memory storage and a `wiremock` mock for the compiler service.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use oprc_cp_storage::traits::StorageFactory;
use oprc_cp_storage::unified::build_memory_factory;
use oprc_pm::{
    config::{DeploymentPolicyConfig, ServerConfig},
    crm::CrmManager,
    server::ApiServer,
    services::{
        DeploymentService, PackageService, ScriptService,
        artifact::MemoryArtifactStore,
        compiler::{CompilerClient, CompilerConfig},
    },
};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Build a test server with in-memory storage and a mock compiler.
async fn build_test_server(compiler_url: &str) -> axum::Router {
    let factory = build_memory_factory();
    let package_storage = Arc::new(factory.create_package_storage());
    let deployment_storage = Arc::new(factory.create_deployment_storage());

    // CRM manager with a dummy URL (no actual gRPC calls in these tests)
    let crm_config = oprc_pm::config::CrmManagerConfig {
        clusters: {
            let mut m = std::collections::HashMap::new();
            m.insert(
                "default".to_string(),
                oprc_pm::config::CrmClientConfig {
                    url: "http://localhost:19999".to_string(),
                    timeout: Some(5),
                    retry_attempts: 1,
                    api_key: None,
                    tls: None,
                },
            );
            m
        },
        default_cluster: Some("default".to_string()),
        health_check_interval: Some(60),
        circuit_breaker: None,
        health_cache_ttl_seconds: 15,
    };
    let crm_manager = Arc::new(CrmManager::new(crm_config).unwrap());

    let policy = DeploymentPolicyConfig {
        max_retries: 1,
        rollback_on_partial: false,
        package_delete_cascade: false,
    };

    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
        policy.clone(),
    ));
    let package_service = Arc::new(PackageService::new(
        package_storage.clone(),
        deployment_service.clone(),
        policy,
    ));

    let artifact_store = Arc::new(MemoryArtifactStore::new());
    let source_store: Arc<MemoryArtifactStore> = artifact_store.clone();

    let compiler = Arc::new(CompilerClient::new(CompilerConfig {
        url: compiler_url.to_string(),
        timeout_seconds: 10,
        max_retries: 0,
    }));

    let script_service = Arc::new(ScriptService::new(
        compiler,
        artifact_store.clone(),
        source_store.clone(),
        package_service.clone(),
        deployment_service.clone(),
        "http://localhost:8080/api/v1/artifacts".to_string(),
    ));

    let server_config = ServerConfig {
        host: "0.0.0.0".to_string(),
        port: 0,
        workers: None,
        static_dir: "/tmp/static-nonexistent".to_string(),
    };

    ApiServer::with_all(
        package_service,
        deployment_service,
        crm_manager,
        server_config,
        None,
        Some(artifact_store.clone()),
        Some(source_store),
        Some(script_service),
    )
    .into_router()
}

/// Helper to make a JSON request and return (status, body_bytes).
async fn json_request(
    app: &axum::Router,
    method_str: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, Vec<u8>) {
    let req = match body {
        Some(b) => Request::builder()
            .method(method_str)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&b).unwrap()))
            .unwrap(),
        None => Request::builder()
            .method(method_str)
            .uri(uri)
            .body(Body::empty())
            .unwrap(),
    };

    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let body_bytes = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, body_bytes)
}

// ---------------------------------------------------------------------------
// Phase 6: Artifact Storage Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_artifact_not_found() {
    let mock_server = MockServer::start().await;
    let app = build_test_server(&mock_server.uri()).await;

    // Valid hex ID but no artifact stored
    let id = "a".repeat(64);
    let (status, body) =
        json_request(&app, "GET", &format!("/api/v1/artifacts/{}", id), None)
            .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(resp["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_get_artifact_invalid_id() {
    let mock_server = MockServer::start().await;
    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) =
        json_request(&app, "GET", "/api/v1/artifacts/invalid-id", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        resp["error"]
            .as_str()
            .unwrap()
            .contains("Invalid artifact ID")
    );
}

// ---------------------------------------------------------------------------
// Phase 7: Script Compile Endpoint Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_compile_script_success() {
    let mock_server = MockServer::start().await;

    // Mock compiler returns WASM bytes
    let fake_wasm = vec![0x00, 0x61, 0x73, 0x6d]; // minimal WASM magic
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm.clone())
                .insert_header("content-type", "application/wasm"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({
            "source": "class Counter { count = 0; }",
            "language": "typescript"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], true);
    assert!(resp["wasm_size"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_compile_script_error() {
    let mock_server = MockServer::start().await;

    // Mock compiler returns 400 with errors
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "success": false,
            "errors": ["Type error at line 5: Cannot find name 'x'"]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({
            "source": "const x: Foo = 1;",
            "language": "typescript"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], false);
    assert!(!resp["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_compile_script_compiler_unavailable() {
    // Point to a non-existent server
    let app = build_test_server("http://localhost:1").await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({
            "source": "class X {}",
            "language": "typescript"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], false);
    assert!(!resp["errors"].as_array().unwrap().is_empty());
    // Error message should mention compiler
    let error_msg = resp["errors"][0].as_str().unwrap();
    assert!(
        error_msg.contains("Compiler")
            || error_msg.contains("compiler")
            || error_msg.contains("connect"),
        "Error should mention compiler issue: {}",
        error_msg
    );
}

// ---------------------------------------------------------------------------
// Phase 7: Script Deploy Endpoint Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_deploy_script_compilation_failure() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "success": false,
            "errors": ["Syntax error"]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": "invalid code @@@@",
            "package_name": "test-pkg",
            "class_key": "Counter"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], false);
    assert!(resp["errors"].as_array().unwrap().len() > 0);
    assert!(
        resp["message"]
            .as_str()
            .unwrap()
            .contains("Compilation failed")
    );
}

#[tokio::test]
async fn test_deploy_script_stores_artifact_and_source() {
    let mock_server = MockServer::start().await;

    let fake_wasm = b"\x00asm\x01\x00\x00\x00".to_vec();
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm.clone())
                .insert_header("content-type", "application/wasm"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let source_code = r#"
import { service, method, OaaSObject } from "@oaas/sdk";

@service("Counter", { package: "my-pkg" })
class Counter extends OaaSObject {
    count: number = 0;

    @method()
    async increment(): Promise<number> {
        this.count += 1;
        return this.count;
    }
}
export default Counter;
"#;

    let (_status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": source_code,
            "package_name": "my-pkg",
            "class_key": "Counter",
            "function_bindings": [
                {"name": "increment", "stateless": false}
            ]
        })),
    )
    .await;

    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Deployment may fail due to CRM being unavailable, but artifact should be stored
    assert!(
        resp["artifact_id"].is_string(),
        "Artifact ID should be present: {:?}",
        resp
    );
    assert!(
        resp["artifact_url"].is_string(),
        "Artifact URL should be present: {:?}",
        resp
    );

    // Verify the artifact URL points to our artifacts endpoint
    let artifact_url = resp["artifact_url"].as_str().unwrap();
    assert!(
        artifact_url.contains("/api/v1/artifacts/"),
        "URL should contain artifacts path: {}",
        artifact_url
    );
}

// ---------------------------------------------------------------------------
// Phase 7: Script Source GET Endpoint Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_script_source_not_found() {
    let mock_server = MockServer::start().await;
    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) =
        json_request(&app, "GET", "/api/v1/scripts/nonexistent/counter", None)
            .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(resp["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_deploy_then_get_source() {
    let mock_server = MockServer::start().await;

    let fake_wasm = b"\x00asm\x01\x00\x00\x00".to_vec();
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm)
                .insert_header("content-type", "application/wasm"),
        )
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let source_code = "class SimpleService { x = 42; }";

    // Deploy first
    let (_status, _body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": source_code,
            "package_name": "src-test",
            "class_key": "SimpleService"
        })),
    )
    .await;

    // Now fetch source
    let (status, body) = json_request(
        &app,
        "GET",
        "/api/v1/scripts/src-test/SimpleService",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["package"], "src-test");
    assert_eq!(resp["function"], "SimpleService");
    assert_eq!(resp["source"], source_code);
    assert_eq!(resp["language"], "typescript");
}

// ---------------------------------------------------------------------------
// Deploy creates correct package structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_deploy_creates_package_with_wasm_function() {
    let mock_server = MockServer::start().await;

    let fake_wasm = b"\x00asm\x01\x00\x00\x00".to_vec();
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm)
                .insert_header("content-type", "application/wasm"),
        )
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    // Deploy
    let (_status, _body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": "class Adder { sum = 0; }",
            "package_name": "math-pkg",
            "class_key": "Adder",
            "function_bindings": [
                {"name": "add", "stateless": false},
                {"name": "reset", "stateless": true}
            ]
        })),
    )
    .await;

    // Verify package was created
    let (status, body) =
        json_request(&app, "GET", "/api/v1/packages/math-pkg", None).await;

    assert_eq!(status, StatusCode::OK);
    let pkg: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(pkg["name"], "math-pkg");

    // Check class exists with correct bindings
    let classes = pkg["classes"].as_array().unwrap();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0]["key"], "Adder");

    let bindings = classes[0]["function_bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0]["name"], "add");
    assert_eq!(bindings[0]["stateless"], false);
    assert_eq!(bindings[1]["name"], "reset");
    assert_eq!(bindings[1]["stateless"], true);

    // Check function exists with WASM type
    let functions = pkg["functions"].as_array().unwrap();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0]["function_type"], "WASM");
    assert!(
        functions[0]["provision_config"]["wasm_module_url"]
            .as_str()
            .unwrap()
            .contains("/api/v1/artifacts/")
    );
}

// ---------------------------------------------------------------------------
// Deploy with default bindings (no explicit function_bindings)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_deploy_default_bindings() {
    let mock_server = MockServer::start().await;

    let fake_wasm = b"\x00asm".to_vec();
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm)
                .insert_header("content-type", "application/wasm"),
        )
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    // Deploy without bindings
    let (_status, _body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": "class Worker {}",
            "package_name": "worker-pkg",
            "class_key": "Worker"
        })),
    )
    .await;

    // Package should have default binding
    let (status, body) =
        json_request(&app, "GET", "/api/v1/packages/worker-pkg", None).await;

    assert_eq!(status, StatusCode::OK);
    let pkg: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bindings = pkg["classes"][0]["function_bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0]["name"], "Worker");
}

// ---------------------------------------------------------------------------
// Redeploy (update existing package)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_redeploy_updates_package() {
    let mock_server = MockServer::start().await;

    let fake_wasm = b"\x00asm-v1".to_vec();
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm)
                .insert_header("content-type", "application/wasm"),
        )
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let source_v1 = "class Counter { count = 0; }";
    let source_v2 = "class Counter { count = 0; history = []; }";

    // Deploy v1
    let (_s, _b) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": source_v1,
            "package_name": "counter-pkg",
            "class_key": "Counter"
        })),
    )
    .await;

    // Deploy v2 (same package/class)
    let (_s, _b) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": source_v2,
            "package_name": "counter-pkg",
            "class_key": "Counter"
        })),
    )
    .await;

    // Source should be v2 now
    let (status, body) =
        json_request(&app, "GET", "/api/v1/scripts/counter-pkg/Counter", None)
            .await;

    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["source"], source_v2);
}

// ---------------------------------------------------------------------------
// Script service not configured (graceful error)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_script_endpoints_without_service_configured() {
    // Build server without script service
    let factory = build_memory_factory();
    let package_storage = Arc::new(factory.create_package_storage());
    let deployment_storage = Arc::new(factory.create_deployment_storage());
    let crm_config = oprc_pm::config::CrmManagerConfig {
        clusters: {
            let mut m = std::collections::HashMap::new();
            m.insert(
                "default".to_string(),
                oprc_pm::config::CrmClientConfig {
                    url: "http://localhost:19999".to_string(),
                    timeout: Some(5),
                    retry_attempts: 1,
                    api_key: None,
                    tls: None,
                },
            );
            m
        },
        default_cluster: Some("default".to_string()),
        health_check_interval: Some(60),
        circuit_breaker: None,
        health_cache_ttl_seconds: 15,
    };
    let crm_manager = Arc::new(CrmManager::new(crm_config).unwrap());
    let policy = DeploymentPolicyConfig {
        max_retries: 0,
        rollback_on_partial: false,
        package_delete_cascade: false,
    };
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage,
        crm_manager.clone(),
        policy.clone(),
    ));
    let package_service = Arc::new(PackageService::new(
        package_storage,
        deployment_service.clone(),
        policy,
    ));

    let config = ServerConfig {
        host: "0.0.0.0".to_string(),
        port: 0,
        workers: None,
        static_dir: "/tmp/static-nonexistent".to_string(),
    };

    // Use with_gateway (no scripts) — fields will be None
    let app = ApiServer::with_gateway(
        package_service,
        deployment_service,
        crm_manager,
        config,
        None,
    )
    .into_router();

    // Compile should return 503
    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({"source": "x"})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(resp["error"].as_str().unwrap().contains("not configured"));

    // Deploy should return 503
    let (status, _) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": "x",
            "package_name": "p",
            "class_key": "c"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    // Get source should return 503
    let (status, _) =
        json_request(&app, "GET", "/api/v1/scripts/p/f", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    // Get artifact should return 503
    let id = "a".repeat(64);
    let (status, _) =
        json_request(&app, "GET", &format!("/api/v1/artifacts/{}", id), None)
            .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

// ---------------------------------------------------------------------------
// Compile with default language (omitted field)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_compile_omit_language_defaults_to_typescript() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(vec![0u8; 4])
                .insert_header("content-type", "application/wasm"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    // No "language" field — should default to "typescript"
    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({ "source": "const x = 1;" })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], true);
}

// ---------------------------------------------------------------------------
// Health check still works
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_check_with_scripts() {
    let mock_server = MockServer::start().await;
    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) = json_request(&app, "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["status"], "healthy");
}

// ---------------------------------------------------------------------------
// Compiler 413 Payload Too Large handling
// ---------------------------------------------------------------------------

/// When the compiler service returns 413 (Fastify bodyLimit exceeded),
/// the PM should surface this as a compile error, not crash or hang.
#[tokio::test]
async fn test_compile_handles_compiler_413_payload_too_large() {
    let mock_server = MockServer::start().await;

    // Simulate Fastify's 413 response
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(ResponseTemplate::new(413).set_body_json(json!({
            "statusCode": 413,
            "code": "FST_ERR_CTP_BODY_TOO_LARGE",
            "error": "Payload Too Large",
            "message": "Request body is too large"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({
            "source": "class X {}",
            "language": "typescript"
        })),
    )
    .await;

    // PM should return BAD_REQUEST (400) with success: false
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "PM should map compiler 413 to 400"
    );
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], false);
    let errors = resp["errors"].as_array().unwrap();
    assert!(!errors.is_empty(), "Should have error message");
    let error_msg = errors[0].as_str().unwrap();
    assert!(
        error_msg.contains("413")
            || error_msg.contains("Payload Too Large")
            || error_msg.contains("too large"),
        "Error should mention payload size issue: {}",
        error_msg
    );
}

/// Deploy endpoint should also handle compiler 413 gracefully.
#[tokio::test]
async fn test_deploy_handles_compiler_413_payload_too_large() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(ResponseTemplate::new(413).set_body_json(json!({
            "statusCode": 413,
            "code": "FST_ERR_CTP_BODY_TOO_LARGE",
            "error": "Payload Too Large",
            "message": "Request body is too large"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/deploy",
        Some(json!({
            "source": "class X {}",
            "package_name": "test-pkg",
            "class_key": "TestClass",
            "language": "typescript"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], false);
    assert!(!resp["errors"].as_array().unwrap().is_empty());
}

/// The PM should accept large source payloads (the DefaultBodyLimit is 50 MB)
/// and forward them to the compiler without its own 413.
#[tokio::test]
async fn test_compile_accepts_large_source_payload() {
    let mock_server = MockServer::start().await;

    // Compiler accepts and returns WASM
    let fake_wasm = vec![0x00, 0x61, 0x73, 0x6d];
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm)
                .insert_header("content-type", "application/wasm"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    // Generate a source > 1 MB to ensure PM doesn't block it
    let big_comment =
        "// ".to_string() + &"x".repeat(1_500_000) + "\nclass X {}";

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({
            "source": big_comment,
            "language": "typescript"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "PM should accept payloads > 1 MB");
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], true);
}

/// Verify that compiler receives the full source body (not truncated).
#[tokio::test]
async fn test_compile_forwards_full_source_to_compiler() {
    let mock_server = MockServer::start().await;

    let fake_wasm = vec![0x00, 0x61, 0x73, 0x6d];
    Mock::given(method("POST"))
        .and(path("/compile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(fake_wasm)
                .insert_header("content-type", "application/wasm"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let app = build_test_server(&mock_server.uri()).await;

    // A realistic TypeScript source with imports, decorators, multiple methods
    let realistic_source = r#"
import { service, method, OaaSObject, OaaSError } from "@oaas/sdk";

@service("TsCounter", { package: "e2e-ts-test" })
class Counter extends OaaSObject {
    count: number = 0;
    history: string[] = [];

    @method()
    async increment(amount: number = 1): Promise<number> {
        this.count += amount;
        this.history.push(`+${amount}`);
        this.log("info", `Counter incremented by ${amount} → ${this.count}`);
        return this.count;
    }

    @method()
    async getCount(): Promise<number> {
        return this.count;
    }

    @method()
    async reset(): Promise<number> {
        const old = this.count;
        this.count = 0;
        this.history = [];
        this.log("info", `Counter reset from ${old}`);
        return old;
    }

    @method({ stateless: true })
    async echo(data: any): Promise<any> {
        return data;
    }

    @method()
    async failOnPurpose(message: string = "intentional error"): Promise<void> {
        throw new OaaSError(message);
    }
}

export default Counter;
"#;

    let source_len = realistic_source.len();

    let (status, body) = json_request(
        &app,
        "POST",
        "/api/v1/scripts/compile",
        Some(json!({
            "source": realistic_source,
            "language": "typescript"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let resp: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp["success"], true);

    // Verify wiremock received the request (expect(1) will fail on drop if not)
    // Also verify the source was sent in full
    let received = mock_server.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        1,
        "Compiler should have received one request"
    );
    let received_body: serde_json::Value =
        serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(
        received_body["source"].as_str().unwrap().len(),
        source_len,
        "Source should not be truncated"
    );
}
