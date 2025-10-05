use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep; // for PredicateBooleanExt

// ODGM imports
use oprc_grpc::CreateCollectionRequest;
use oprc_odgm::OdgmConfig;

// Spin up an in-process ODGM gRPC server on a random port and create a test collection.
async fn start_odgm_with_collection() -> (String, String) {
    // Pick a random port by binding to port 0 first
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("bind random");
    let port = listener.local_addr().unwrap().port();
    drop(listener); // free it so ODGM can bind

    let mut cfg = OdgmConfig::default();
    cfg.node_id = Some(1);
    cfg.members = Some("1".into());
    cfg.http_port = port;
    cfg.events_enabled = false; // events not needed for these tests
    cfg.max_sessions = 1;

    // Start ODGM server (spawns gRPC service internally)
    let (odgm, _pool) =
        oprc_odgm::start_server(&cfg).await.expect("start odgm");

    // Create a collection (class) we will target
    let collection = format!("cli_obj_str_{}", nanoid::nanoid!(6));
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

    // Give shards time to initialize
    sleep(Duration::from_millis(300)).await;

    (format!("http://127.0.0.1:{}", port), collection)
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_object_setstr_getstr_roundtrip() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    // 1) SetStr object with two string entries
    let mut cmd = Command::cargo_bin("oprc-cli").expect("binary built");
    let assert = cmd
        .arg("object")
        .arg("setstr")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--object-id-str")
        .arg("user-alpha")
        .arg("-s")
        .arg("name=alice")
        .arg("-s")
        .arg("role=admin")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert();
    assert.success();

    // 2) GetStr specific string key
    let mut cmd2 = Command::cargo_bin("oprc-cli").expect("binary built");
    let assert2 = cmd2
        .arg("object")
        .arg("getstr")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--object-id-str")
        .arg("user-alpha")
        .arg("--key-str")
        .arg("name")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert();
    assert2.success().stdout(predicates::str::contains("alice"));

    // 3) GetStr numeric key path should still work when absent (prints full object); just ensure success
    let mut cmd3 = Command::cargo_bin("oprc-cli").expect("binary built");
    let assert3 = cmd3
        .arg("object")
        .arg("getstr")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--object-id-str")
        .arg("user-alpha")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert();
    assert3.success();
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_object_setstr_duplicate_fails() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    // First create succeeds
    let mut cmd = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd.arg("object")
        .arg("setstr")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--object-id-str")
        .arg("dup-user")
        .arg("-s")
        .arg("k=v")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .success();

    // Second create with same string id should fail (AlreadyExists -> panic in CLI expect path)
    let mut cmd2 = Command::cargo_bin("oprc-cli").expect("binary built");
    cmd2.arg("object")
        .arg("setstr")
        .arg("--cls-id")
        .arg(&collection)
        .arg("0")
        .arg("--object-id-str")
        .arg("dup-user")
        .arg("-s")
        .arg("k=other")
        .arg("--grpc-url")
        .arg(&grpc_url)
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("AlreadyExists")
                .or(predicate::str::contains("object already exists")),
        );
}
