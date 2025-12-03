//! CLI tests for `oprc-cli object list` command (ListObjects API).
//!
//! Tests both gRPC and Zenoh backends for listing objects in a partition.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

use oprc_grpc::CreateCollectionRequest;
use oprc_odgm::OdgmConfig;

/// Start an in-process ODGM gRPC server and create a test collection.
/// Returns (grpc_url, collection_name).
async fn start_odgm_with_collection() -> (String, String) {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("bind random");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut cfg = OdgmConfig::default();
    cfg.node_id = Some(1);
    cfg.members = Some("1".into());
    cfg.http_port = port;
    cfg.events_enabled = false;
    cfg.max_sessions = 1;

    let (odgm, _pool) = oprc_odgm::start_server(&cfg, None)
        .await
        .expect("start odgm");

    let collection = format!("cli_list_{}", nanoid::nanoid!(8));
    let req = CreateCollectionRequest {
        name: collection.clone(),
        partition_count: 1,
        replica_count: 1,
        shard_type: "basic".to_string(),
        shard_assignments: vec![],
        options: std::collections::HashMap::new(),
        invocations: None,
    };
    odgm.metadata_manager
        .create_collection(req)
        .await
        .expect("create collection");

    sleep(Duration::from_millis(300)).await;

    (format!("http://127.0.0.1:{}", port), collection)
}

/// Helper to create an object via CLI.
fn create_object(grpc_url: &str, collection: &str, object_id: &str) {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("set")
        .arg("--cls-id")
        .arg(collection)
        .arg("0")
        .arg(object_id)
        .arg("-s")
        .arg("key=value")
        .arg("--grpc-url")
        .arg(grpc_url)
        .assert()
        .success();
}

// =============================================================================
// gRPC List Tests
// =============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_empty_collection() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("0 object(s)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_basic() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    // Create some objects
    create_object(&grpc_url, &collection, "obj-001");
    create_object(&grpc_url, &collection, "obj-002");
    create_object(&grpc_url, &collection, "obj-003");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("obj-001"))
        .stdout(predicate::str::contains("obj-002"))
        .stdout(predicate::str::contains("obj-003"))
        .stdout(predicate::str::contains("3 object(s)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_with_prefix_filter() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    create_object(&grpc_url, &collection, "user-alice");
    create_object(&grpc_url, &collection, "user-bob");
    create_object(&grpc_url, &collection, "item-sword");
    create_object(&grpc_url, &collection, "item-shield");

    // Filter by "user-" prefix
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--prefix")
        .arg("user-")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("user-alice"))
        .stdout(predicate::str::contains("user-bob"))
        .stdout(predicate::str::contains("2 object(s)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_with_limit() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    // Create 5 objects
    for i in 0..5 {
        create_object(&grpc_url, &collection, &format!("limited-{:03}", i));
    }

    // Request limit of 2
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--limit")
        .arg("2")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("2 object(s)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_json_output() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    create_object(&grpc_url, &collection, "json-test-obj");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    let output = cmd
        .arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--json")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .output()
        .expect("execute command");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("valid JSON");
    assert!(json.get("objects").is_some());
    assert!(json.get("count").is_some());
    assert_eq!(json["count"], 1);
    assert!(
        json["objects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["object_id"] == "json-test-obj")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_shows_version_and_entry_count() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    create_object(&grpc_url, &collection, "meta-obj");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        // Output format: "meta-obj  v1  entries=1"
        .stdout(predicate::str::contains("meta-obj"))
        .stdout(predicate::str::contains("v"))
        .stdout(predicate::str::contains("entries="));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_alias_ls_works() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    create_object(&grpc_url, &collection, "alias-test");

    // Test "ls" alias
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("ls")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("alias-test"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_alias_l_works() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    create_object(&grpc_url, &collection, "alias-l-test");

    // Test "l" alias
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("l")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("alias-l-test"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_list_objects_prefix_no_match() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    create_object(&grpc_url, &collection, "real-object");

    // Filter by prefix that doesn't match anything
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("object")
        .arg("list")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--prefix")
        .arg("nonexistent-")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("0 object(s)"));
}
