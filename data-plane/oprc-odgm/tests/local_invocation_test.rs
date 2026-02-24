//! Integration tests for local (in-process) function invocation.
//!
//! Validates that an `ObjectUnifiedShard` with a `LocalFnOffloader` executes
//! functions in-process, which is the foundation for WASM runtime integration.

use std::sync::Arc;

use oprc_grpc::{InvocationRequest, ObjectInvocationRequest, ResponseStatus};
use oprc_invoke::local::LocalFnOffloader;
use oprc_odgm::shard::object_trait::ObjectShard;
use oprc_odgm::shard::{ShardBuilder, ShardOptions};
use oprc_odgm::shard::traits::ShardMetadata;

fn test_metadata() -> ShardMetadata {
    ShardMetadata {
        id: 1,
        collection: "local_test".into(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "basic".into(),
        options: Default::default(),
        invocations: Default::default(),
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}

async fn shard_with_local_offloader(
    offloader: LocalFnOffloader,
) -> impl ObjectShard {
    ShardBuilder::new()
        .metadata(test_metadata())
        .options(ShardOptions::new(160, 256))
        .memory_storage()
        .expect("memory storage")
        .no_replication()
        .build_minimal()
        .await
        .expect("shard")
        .with_local_offloader(Arc::new(offloader))
}

/// Local echo function returns the payload unchanged.
#[tokio::test]
async fn local_invoke_fn_echo() {
    let mut offloader = LocalFnOffloader::new();
    offloader.register_fn("echo", |p| p);

    let shard = shard_with_local_offloader(offloader).await;
    shard.initialize().await.expect("init");

    let req = InvocationRequest {
        cls_id: "local_test".to_string(),
        fn_id: "echo".to_string(),
        payload: b"hello-world".to_vec(),
        ..Default::default()
    };
    let resp = shard.invoke_fn(req).await.expect("invoke_fn failed");
    assert_eq!(resp.status, ResponseStatus::Okay as i32);
    assert_eq!(resp.payload, Some(b"hello-world".to_vec()));
}

/// Calling an unknown function returns an error.
#[tokio::test]
async fn local_invoke_fn_unknown_returns_error() {
    let shard = shard_with_local_offloader(LocalFnOffloader::new()).await;
    shard.initialize().await.expect("init");

    let req = InvocationRequest {
        cls_id: "local_test".to_string(),
        fn_id: "does_not_exist".to_string(),
        payload: vec![],
        ..Default::default()
    };
    assert!(shard.invoke_fn(req).await.is_err());
}

/// A transform function processes the payload before returning it.
#[tokio::test]
async fn local_invoke_fn_transform() {
    let mut offloader = LocalFnOffloader::new();
    offloader.register_fn("upper", |p| {
        String::from_utf8_lossy(&p).to_uppercase().into_bytes()
    });

    let shard = shard_with_local_offloader(offloader).await;
    shard.initialize().await.expect("init");

    let req = InvocationRequest {
        cls_id: "local_test".to_string(),
        fn_id: "upper".to_string(),
        payload: b"hello".to_vec(),
        ..Default::default()
    };
    let resp = shard.invoke_fn(req).await.expect("invoke_fn failed");
    assert_eq!(resp.payload, Some(b"HELLO".to_vec()));
}

/// Object invocation falls back to the stateless fn handler.
#[tokio::test]
async fn local_invoke_obj_fallback_to_fn_handler() {
    let mut offloader = LocalFnOffloader::new();
    offloader.register_fn("process", |mut p| {
        p.extend_from_slice(b"-ok");
        p
    });

    let shard = shard_with_local_offloader(offloader).await;
    shard.initialize().await.expect("init");

    let req = ObjectInvocationRequest {
        cls_id: "local_test".to_string(),
        fn_id: "process".to_string(),
        payload: b"data".to_vec(),
        partition_id: 0,
        object_id: Some("obj-1".to_string()),
        ..Default::default()
    };
    let resp = shard.invoke_obj(req).await.expect("invoke_obj failed");
    assert_eq!(resp.status, ResponseStatus::Okay as i32);
    assert_eq!(resp.payload, Some(b"data-ok".to_vec()));
}

/// Dedicated object handler takes precedence over the stateless handler.
#[tokio::test]
async fn local_invoke_obj_prefers_dedicated_obj_handler() {
    let mut offloader = LocalFnOffloader::new();
    offloader.register_fn("fn", |_| b"from-fn".to_vec());
    offloader.register_obj_fn("fn", |_| b"from-obj".to_vec());

    let shard = shard_with_local_offloader(offloader).await;
    shard.initialize().await.expect("init");

    let req = ObjectInvocationRequest {
        cls_id: "local_test".to_string(),
        fn_id: "fn".to_string(),
        payload: vec![],
        partition_id: 0,
        object_id: Some("any".to_string()),
        ..Default::default()
    };
    let resp = shard.invoke_obj(req).await.expect("invoke_obj failed");
    assert_eq!(resp.payload, Some(b"from-obj".to_vec()));
}

/// Without any offloader the shard returns a configuration error.
#[tokio::test]
async fn invoke_fn_without_offloader_returns_configuration_error() {
    let shard = ShardBuilder::new()
        .metadata(test_metadata())
        .options(ShardOptions::new(160, 256))
        .memory_storage()
        .expect("storage")
        .no_replication()
        .build_minimal()
        .await
        .expect("shard");

    shard.initialize().await.expect("init");

    let req = InvocationRequest {
        cls_id: "local_test".to_string(),
        fn_id: "echo".to_string(),
        payload: vec![],
        ..Default::default()
    };
    assert!(shard.invoke_fn(req).await.is_err());
}
