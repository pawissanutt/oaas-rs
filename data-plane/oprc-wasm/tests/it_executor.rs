use oprc_wasm::executor::{WasmInvocationExecutor, WasmResponseStatus};
use oprc_wasm::host::OdgmDataOps;
use oprc_wasm::mock_ops::MockDataOps;
use oprc_wasm::store::WasmModuleStore;
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

#[tokio::test]
async fn test_invoke_fn_echo() {
    let mut config = wasmtime::Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    config.consume_fuel(true);
    let engine = wasmtime::Engine::new(&config).unwrap();

    let store = Arc::new(WasmModuleStore::new(engine));

    // Load the pre-compiled guest echo component
    let wasm_bytes = fs::read(wasm_guest_path("wasm_guest_echo"))
        .await
        .expect("Guest component not found. Run: cargo build -p wasm-guest-echo --target wasm32-wasip2 --release");

    store.load_from_bytes("echo-fn", &wasm_bytes).await.unwrap();

    let executor = WasmInvocationExecutor::new(store).unwrap();
    let data_ops = Box::new(MockDataOps::default());

    let payload = Some(b"hello wasm".to_vec());
    let response = executor
        .invoke_fn("echo-fn", "test-class", 0, payload.clone(), data_ops, None)
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
async fn test_invoke_obj_transform() {
    let mut config = wasmtime::Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    config.consume_fuel(true);
    let engine = wasmtime::Engine::new(&config).unwrap();

    let store = Arc::new(WasmModuleStore::new(engine));

    let wasm_bytes = fs::read(wasm_guest_path("wasm_guest_echo"))
        .await
        .expect("Guest component not found.");

    store.load_from_bytes("echo-fn", &wasm_bytes).await.unwrap();

    let executor = WasmInvocationExecutor::new(store).unwrap();

    // Pre-populate mock data
    let mock_ops = MockDataOps::default();
    let cls_id = "test-class";
    let obj_id = "obj-123";
    let initial_data = b"initial data".to_vec();
    mock_ops
        .set_object(cls_id, 0, obj_id, initial_data.clone())
        .await
        .unwrap();

    let data_ops = Box::new(mock_ops.clone());

    let response = executor
        .invoke_obj("echo-fn", cls_id, 0, obj_id, None, data_ops, None)
        .await
        .unwrap();

    assert_eq!(response.status, WasmResponseStatus::Okay);

    let mut expected_payload = initial_data.clone();
    expected_payload.extend_from_slice(b" - seen by wasm");
    assert_eq!(response.payload, Some(expected_payload.clone()));

    // Verify it was written back to MockDataOps
    let saved_data = mock_ops
        .get_object(cls_id, 0, obj_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved_data, expected_payload);
}
