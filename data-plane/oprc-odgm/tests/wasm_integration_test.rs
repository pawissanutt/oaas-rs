//! Integration test: WASM function invocation through ODGM shard.
//!
//! Validates the full pipeline:
//! 1. Build a shard with a WASM offloader
//! 2. Invoke a stateless function (echo)
//! 3. Invoke an object method (read → transform → write)
//! 4. Verify object state was mutated
//!
//! Requires the wasm-guest-echo component to be pre-built:
//!   cargo build -p wasm-guest-echo --target wasm32-wasip2 --release

#![cfg(feature = "wasm")]

use std::sync::Arc;

use oprc_grpc::{
    FuncInvokeRoute, InvocationRequest, InvocationRoute,
    ObjectInvocationRequest, ResponseStatus,
};
use oprc_odgm::shard::object_trait::ObjectShard;
use oprc_odgm::shard::traits::ShardMetadata;
use oprc_odgm::shard::{ObjectVal, ShardBuilder, ShardOptions};
use oprc_odgm::wasm_bridge;

fn wasm_module_url() -> String {
    format!(
        "file://{}/../../target/wasm32-wasip2/release/wasm_guest_echo.wasm",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn wasm_metadata() -> ShardMetadata {
    let mut fn_routes = std::collections::HashMap::new();
    let module_url = wasm_module_url();
    // fn_id keys must match the guest's handler names (echo, transform)
    // because fn_id is passed as function_name to the guest's on_invoke.
    fn_routes.insert(
        "echo".to_string(),
        FuncInvokeRoute {
            url: "wasm://echo".to_string(),
            stateless: true,
            standby: false,
            active_group: vec![],
            wasm_module_url: Some(module_url.clone()),
        },
    );
    fn_routes.insert(
        "transform".to_string(),
        FuncInvokeRoute {
            url: "wasm://transform".to_string(),
            stateless: false,
            standby: false,
            active_group: vec![],
            wasm_module_url: Some(module_url),
        },
    );
    ShardMetadata {
        id: 100,
        collection: "wasm_test".into(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "basic".into(),
        options: Default::default(),
        invocations: InvocationRoute {
            fn_routes,
            disabled_fn: vec![],
        },
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}

/// Helper: build a minimal shard and attach the WASM offloader.
async fn shard_with_wasm() -> Arc<dyn ObjectShard> {
    let metadata = wasm_metadata();
    let shard = ShardBuilder::new()
        .metadata(metadata.clone())
        .options(ShardOptions::new(160, 256))
        .memory_storage()
        .expect("memory storage")
        .no_replication()
        .build_minimal()
        .await
        .expect("shard");

    shard.initialize().await.expect("init");

    let arc_shard: Arc<dyn ObjectShard> = Arc::new(shard);

    // Setup WASM offloader (detects wasm:// routes, loads modules)
    if let Some(offloader) =
        wasm_bridge::setup_wasm_offloader(&metadata, arc_shard.clone()).await
    {
        arc_shard
            .set_local_offloader(offloader)
            .expect("set offloader");
    } else {
        panic!("Expected WASM offloader to be created from wasm:// routes");
    }

    arc_shard
}

/// Stateless echo: send payload → get it back unchanged.
#[tokio::test]
async fn wasm_invoke_fn_echo() {
    let shard = shard_with_wasm().await;

    let req = InvocationRequest {
        cls_id: "wasm_test".to_string(),
        fn_id: "echo".to_string(),
        payload: b"hello wasm".to_vec(),
        ..Default::default()
    };

    let resp = shard.invoke_fn(req).await.expect("invoke_fn");
    assert_eq!(resp.status, ResponseStatus::Okay as i32);
    assert_eq!(resp.payload, Some(b"hello wasm".to_vec()));
}

/// Object method: read object → append text → write back → verify.
#[tokio::test]
async fn wasm_invoke_obj_transform() {
    let shard = shard_with_wasm().await;

    let obj_id = "obj-42";
    let initial_data = b"initial data".to_vec();

    // Pre-populate: create metadata + _raw entry
    let _ = shard.ensure_metadata_exists(obj_id).await;
    shard
        .set_entry_granular(
            obj_id,
            "_raw",
            ObjectVal {
                data: initial_data.clone(),
                r#type: oprc_grpc::ValType::Byte,
            },
        )
        .await
        .expect("seed _raw entry");

    // Invoke the object method
    let req = ObjectInvocationRequest {
        cls_id: "wasm_test".to_string(),
        fn_id: "transform".to_string(),
        partition_id: 0,
        object_id: Some(obj_id.to_string()),
        payload: vec![],
        ..Default::default()
    };

    let resp = shard.invoke_obj(req).await.expect("invoke_obj");
    assert_eq!(resp.status, ResponseStatus::Okay as i32);

    // The guest appends " - seen by wasm"
    let expected = b"initial data - seen by wasm";
    assert_eq!(resp.payload, Some(expected.to_vec()));

    // Verify the _raw entry was updated in the shard
    let stored = shard
        .get_entry_granular(obj_id, "_raw")
        .await
        .expect("get_entry")
        .expect("entry should exist");
    assert_eq!(stored.data, expected.to_vec());
}

/// Invoking a missing WASM module returns an error.
#[tokio::test]
async fn wasm_invoke_fn_missing_module_returns_error() {
    let shard = shard_with_wasm().await;

    let req = InvocationRequest {
        cls_id: "wasm_test".to_string(),
        fn_id: "nonexistent-fn".to_string(),
        payload: vec![],
        ..Default::default()
    };

    // The executor should return an error because no module is loaded for this fn_id
    let result = shard.invoke_fn(req).await;
    assert!(result.is_err(), "Expected error for missing WASM module");
}
