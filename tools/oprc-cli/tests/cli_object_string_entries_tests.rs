use assert_cmd::prelude::*;
use oprc_grpc::CreateCollectionRequest;
use oprc_odgm::OdgmConfig;
use predicates::prelude::*;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

async fn start_odgm_with_collection() -> (String, String) {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("bind random");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut cfg = OdgmConfig::default();
    cfg.node_id = Some(2);
    cfg.members = Some("2".into());
    cfg.http_port = port;
    cfg.events_enabled = false;
    let (odgm, _pool) =
        oprc_odgm::start_server(&cfg).await.expect("start odgm");
    let collection = format!("cli_obj_str_entries_{}", nanoid::nanoid!(6));
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

#[tokio::test(flavor = "multi_thread")]
async fn cli_object_liststr_and_getstrkey() {
    let (grpc_url, collection) = start_odgm_with_collection().await;

    // Create object with several string entries
    Command::cargo_bin("oprc-cli")
        .expect("bin")
        .args([
            "object",
            "setstr",
            "--cls-id",
            &collection,
            "0",
            "acct-1",
            "-s",
            "owner=alice",
            "-s",
            "region=us-east",
            "-s",
            "tier=premium",
            "--grpc-url",
            &grpc_url,
        ])
        .assert()
        .success();

    // List keys only
    Command::cargo_bin("oprc-cli")
        .expect("bin")
        .args([
            "object",
            "liststr",
            "--cls-id",
            &collection,
            "0",
            "acct-1",
            "--grpc-url",
            &grpc_url,
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("owner")
                .and(predicate::str::contains("region"))
                .and(predicate::str::contains("tier")),
        );

    // List with values
    Command::cargo_bin("oprc-cli")
        .expect("bin")
        .args([
            "object",
            "liststr",
            "--cls-id",
            &collection,
            "0",
            "acct-1",
            "--with-values",
            "--grpc-url",
            &grpc_url,
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("owner=alice")
                .and(predicate::str::contains("region=us-east"))
                .and(predicate::str::contains("tier=premium")),
        );

    // Get single key via explicit subcommand
    Command::cargo_bin("oprc-cli")
        .expect("bin")
        .args([
            "object",
            "getstrkey",
            "--cls-id",
            &collection,
            "0",
            "acct-1",
            "--key-str",
            "region",
            "--grpc-url",
            &grpc_url,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("us-east"));
}
