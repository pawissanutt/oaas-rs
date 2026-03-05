use oprc_wasm::executor::{
    OopContext, WasmInvocationExecutor, WasmResponseStatus,
};
use oprc_wasm::host::OdgmDataOps;
use oprc_wasm::mock_ops::MockDataOps;
use oprc_wasm::store::{WasmModuleStore, WorldType};
use std::path::Path;
use std::sync::Arc;
use tokio::fs;

/// Resolve the workspace root from CARGO_MANIFEST_DIR.
fn wasm_guest_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join(format!("target/wasm32-wasip2/release/{name}.wasm"))
}

fn test_engine() -> wasmtime::Engine {
    let mut config = wasmtime::Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    config.consume_fuel(true);
    wasmtime::Engine::new(&config).unwrap()
}

async fn read_guest_bytes() -> Vec<u8> {
    fs::read(wasm_guest_path("wasm_guest_echo"))
        .await
        .expect("Guest component not found. Run: cargo build -p wasm-guest-echo --target wasm32-wasip2 --release")
}

/// Load the guest component under a given fn_id key.
/// Since fn_id is also passed as function_name to the guest's on_invoke,
/// it must match the handler name in the guest (e.g. "echo", "transform").
async fn load_guest_as(store: &WasmModuleStore, fn_id: &str) {
    let wasm_bytes = read_guest_bytes().await;
    store.load_from_bytes(fn_id, &wasm_bytes).await.unwrap();
}

fn oop_ctx(cls_id: &str, partition_id: u32) -> OopContext {
    OopContext {
        remote_proxy: None,
        shard_cls_id: cls_id.into(),
        shard_partition_id: partition_id,
    }
}

// ─── World Detection ──────────────────────────────────

#[tokio::test]
async fn test_guest_echo_is_oop_world() {
    let store = WasmModuleStore::new(test_engine());
    load_guest_as(&store, "echo").await;
    let module = store.get("echo").await.unwrap();
    // After Phase 9 migration, the guest targets oaas-object world
    assert_eq!(module.world_type, WorldType::ObjectOriented);
}

// ─── Stateless Echo (invoke_fn) ──────────────────────
// fn_id "echo" → guest receives function_name="echo" → echo handler

#[tokio::test]
async fn test_invoke_fn_echo() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "echo").await;

    let executor = WasmInvocationExecutor::new(store).unwrap();
    let data_ops = Box::new(MockDataOps::default());
    let ctx = oop_ctx("test-class", 0);

    let payload = Some(b"hello wasm".to_vec());
    let response = executor
        .invoke_fn(
            "echo",
            "test-class",
            0,
            payload.clone(),
            data_ops,
            Some(&ctx),
        )
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);
    assert_eq!(response.payload, payload);

    let content_type = response
        .headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.as_str());
    assert_eq!(content_type, Some("application/json"));
}

#[tokio::test]
async fn test_invoke_fn_echo_empty_payload() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "echo").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();
    let data_ops = Box::new(MockDataOps::default());
    let ctx = oop_ctx("test-class", 0);

    let response = executor
        .invoke_fn("echo", "test-class", 0, None, data_ops, Some(&ctx))
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);
    assert_eq!(response.payload, None);
}

// ─── Object Method (invoke_obj) — Transform ─────────
// fn_id "transform" → guest receives function_name="transform"

#[tokio::test]
async fn test_invoke_obj_transform_raw_key() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "transform").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();

    // Pre-populate with "_raw" key (legacy fallback path)
    let mock_ops = MockDataOps::default();
    let cls_id = "test-class";
    let obj_id = "obj-123";
    let initial_data = b"initial data".to_vec();
    mock_ops
        .set_value(cls_id, 0, obj_id, "_raw", initial_data.clone())
        .await
        .unwrap();

    let data_ops = Box::new(mock_ops.clone());
    let ctx = oop_ctx(cls_id, 0);

    let response = executor
        .invoke_obj("transform", cls_id, 0, obj_id, None, data_ops, Some(&ctx))
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);

    let mut expected = initial_data;
    expected.extend_from_slice(b" - seen by wasm");
    assert_eq!(response.payload, Some(expected.clone()));

    // Verify written back via set_value to "_raw" key
    let saved = mock_ops
        .get_value(cls_id, 0, obj_id, "_raw")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved, expected);
}

#[tokio::test]
async fn test_invoke_obj_transform_data_key() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "transform").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();

    // Pre-populate with "data" key (preferred key in the updated guest)
    let mock_ops = MockDataOps::default();
    let cls_id = "test-class";
    let obj_id = "obj-data-key";
    let initial_data = b"some data".to_vec();
    mock_ops
        .set_value(cls_id, 0, obj_id, "data", initial_data.clone())
        .await
        .unwrap();

    let data_ops = Box::new(mock_ops.clone());
    let ctx = oop_ctx(cls_id, 0);

    let response = executor
        .invoke_obj("transform", cls_id, 0, obj_id, None, data_ops, Some(&ctx))
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);

    let mut expected = initial_data;
    expected.extend_from_slice(b" - seen by wasm");
    assert_eq!(response.payload, Some(expected.clone()));

    // Verify written to "data" key
    let saved = mock_ops
        .get_value(cls_id, 0, obj_id, "data")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved, expected);
}

#[tokio::test]
async fn test_invoke_obj_transform_missing_object() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "transform").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();

    // No data set for this object
    let mock_ops = MockDataOps::default();
    let data_ops = Box::new(mock_ops);
    let ctx = oop_ctx("test-class", 0);

    let response = executor
        .invoke_obj(
            "transform",
            "test-class",
            0,
            "nonexistent",
            None,
            data_ops,
            Some(&ctx),
        )
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::AppError);
    let payload_str = String::from_utf8(response.payload.unwrap()).unwrap();
    assert!(payload_str.contains("no 'data' or '_raw' field"));
}

// ─── Object Ref (get_ref function) ──────────────────
// fn_id "get_ref" → guest returns "cls/partition_id/object_id"

#[tokio::test]
async fn test_invoke_obj_get_ref() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "get_ref").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();

    let cls_id = "my-class";
    let partition_id = 42u32;
    let obj_id = "ref-test-obj";

    let data_ops = Box::new(MockDataOps::default());
    let ctx = oop_ctx(cls_id, partition_id);

    let response = executor
        .invoke_obj(
            "get_ref",
            cls_id,
            partition_id,
            obj_id,
            None,
            data_ops,
            Some(&ctx),
        )
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);
    let ref_str = String::from_utf8(response.payload.unwrap()).unwrap();
    assert_eq!(ref_str, format!("{cls_id}/{partition_id}/{obj_id}"));
}

// ─── Cross-Object Access ────────────────────────────
// fn_id "cross_access" → guest reads a field from a different object
// Note: both objects must be in the same shard (cls + partition) to stay local,
// since we have no remote Zenoh proxy in unit tests.

#[tokio::test]
async fn test_invoke_fn_cross_access() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "cross_access").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();

    let mock_ops = MockDataOps::default();
    let cls = "my-class";
    let partition = 0u32;
    let target_obj = "other-obj";
    let target_data = b"shared data".to_vec();
    // Pre-populate the target object's "data" field (same shard → local access)
    mock_ops
        .set_value(cls, partition, target_obj, "data", target_data.clone())
        .await
        .unwrap();

    let data_ops = Box::new(mock_ops);
    let ctx = oop_ctx(cls, partition);

    // Payload is the ref string "cls/partition/id"
    let ref_str = format!("{cls}/{partition}/{target_obj}");
    let response = executor
        .invoke_fn(
            "cross_access",
            cls,
            partition,
            Some(ref_str.clone().into_bytes()),
            data_ops,
            Some(&ctx),
        )
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);
    assert_eq!(response.payload, Some(target_data));

    // Check the x-source-ref header
    let source_ref = response
        .headers
        .iter()
        .find(|(k, _)| k == "x-source-ref")
        .map(|(_, v)| v.as_str());
    assert_eq!(source_ref, Some(ref_str.as_str()));
}

#[tokio::test]
async fn test_invoke_fn_cross_access_missing_payload() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    load_guest_as(&store, "cross_access").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();

    let data_ops = Box::new(MockDataOps::default());
    let ctx = oop_ctx("my-class", 0);

    // No payload → guest should return InvalidRequest
    let response = executor
        .invoke_fn("cross_access", "my-class", 0, None, data_ops, Some(&ctx))
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::InvalidRequest);
    let payload_str = String::from_utf8(response.payload.unwrap()).unwrap();
    assert!(payload_str.contains("Payload required"));
}

// ─── Unknown Function Name ──────────────────────────
// Load module under a key that doesn't match any handler

#[tokio::test]
async fn test_invoke_fn_unknown_function_name() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    // Load under "unknown_func" — this becomes the function_name in on_invoke
    load_guest_as(&store, "unknown_func").await;
    let executor = WasmInvocationExecutor::new(store).unwrap();
    let data_ops = Box::new(MockDataOps::default());
    let ctx = oop_ctx("test-class", 0);

    let response = executor
        .invoke_fn(
            "unknown_func",
            "test-class",
            0,
            Some(b"test".to_vec()),
            data_ops,
            Some(&ctx),
        )
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::InvalidRequest);
    let payload_str = String::from_utf8(response.payload.unwrap()).unwrap();
    assert!(payload_str.contains("Unknown function: unknown_func"));
}

// ─── Module Not Found ───────────────────────────────

#[tokio::test]
async fn test_invoke_fn_module_not_found() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    let executor = WasmInvocationExecutor::new(store).unwrap();
    let data_ops = Box::new(MockDataOps::default());

    let result = executor
        .invoke_fn("nonexistent", "cls", 0, None, data_ops, None)
        .await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("not loaded"));
}

#[tokio::test]
async fn test_invoke_obj_module_not_found() {
    let store = Arc::new(WasmModuleStore::new(test_engine()));
    let executor = WasmInvocationExecutor::new(store).unwrap();
    let data_ops = Box::new(MockDataOps::default());

    let result = executor
        .invoke_obj("nonexistent", "cls", 0, "obj", None, data_ops, None)
        .await;
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("not loaded"));
}

// ─── Module Store Tests ─────────────────────────────

#[tokio::test]
async fn test_module_store_load_and_retrieve() {
    let store = WasmModuleStore::new(test_engine());
    load_guest_as(&store, "echo").await;

    assert_eq!(store.len().await, 1);
    assert!(store.get("echo").await.is_some());
    assert!(store.get("nonexistent").await.is_none());
}

#[tokio::test]
async fn test_module_store_multiple_keys_same_component() {
    let store = WasmModuleStore::new(test_engine());
    let wasm_bytes = read_guest_bytes().await;

    // Load same component under different fn_id keys
    store.load_from_bytes("echo", &wasm_bytes).await.unwrap();
    store
        .load_from_bytes("transform", &wasm_bytes)
        .await
        .unwrap();
    store.load_from_bytes("get_ref", &wasm_bytes).await.unwrap();

    assert_eq!(store.len().await, 3);
    assert!(store.get("echo").await.is_some());
    assert!(store.get("transform").await.is_some());
    assert!(store.get("get_ref").await.is_some());
}

#[tokio::test]
async fn test_module_store_remove_and_reload() {
    let store = WasmModuleStore::new(test_engine());
    load_guest_as(&store, "echo").await;

    assert!(store.remove("echo").await);
    assert!(store.is_empty().await);

    // Re-load works
    load_guest_as(&store, "echo").await;
    assert_eq!(store.len().await, 1);
}

#[tokio::test]
async fn test_module_store_remove_nonexistent() {
    let store = WasmModuleStore::new(test_engine());
    assert!(!store.remove("does-not-exist").await);
}
