use assert_cmd::prelude::*; // traits for assertions
// Use the cargo_bin! macro to locate the compiled binary
use oprc_odgm::OdgmConfig;
use std::process::Command;
use tokio::time::{Duration, sleep};

async fn start_odgm() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut cfg = OdgmConfig::default();
    cfg.http_port = port;
    cfg.node_id = Some(1);
    cfg.members = Some("1".into());
    let (_srv, _pool) = oprc_odgm::start_server(&cfg, None).await.unwrap();
    // give server time
    sleep(Duration::from_millis(200)).await;
    format!("http://127.0.0.1:{}", port)
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_capabilities_plain_and_json() {
    let url = start_odgm().await;
    // Plain output
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd.arg("capabilities").arg("--grpc-url").arg(&url);
    cmd.assert()
        .success()
        .stdout(predicates::str::contains("string_ids: true"));

    // JSON output
    let mut cmd2 = Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"));
    cmd2.arg("capabilities")
        .arg("--grpc-url")
        .arg(&url)
        .arg("--json");
    cmd2.assert()
        .success()
        .stdout(predicates::str::contains("\"string_ids\": true"));
}
