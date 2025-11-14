mod common;

use std::time::Duration;

use common::{TestConfig, TestEnvironment};
use std::env;
use std::net::TcpListener;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread")]
async fn shard_declares_liveliness_token() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    // Allocate a dedicated Zenoh base port to avoid conflicts with other running services
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind temp port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    unsafe {
        env::set_var("OPRC_ZENOH_PORT", port.to_string());
    }
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("live_coll")
        .await
        .expect("create collection");

    // Allow some time for network start & liveliness token declaration
    sleep(Duration::from_millis(300)).await;

    let session = env.get_session().await;
    let selector = format!("oprc/{}/{}/liveliness/*", "live_coll", 0u16);
    let result = session
        .liveliness()
        .get(selector)
        .await
        .expect("liveliness get should succeed");

    let mut found = false;
    while let Ok(reply) = result.recv_async().await {
        match reply.result() {
            Ok(sample) => {
                let key = sample.key_expr().to_string();
                // Shard IDs for first created collection start at 1
                if key.ends_with("/1") {
                    found = true;
                    break;
                }
            }
            Err(_) => break,
        }
    }

    assert!(found, "expected liveliness token for shard id 1");
}
