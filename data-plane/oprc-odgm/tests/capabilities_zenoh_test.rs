mod common;

use common::{TestConfig, TestEnvironment};
use serde_json::Value;
use std::convert::TryInto;
use zenoh::key_expr::KeyExpr;

const PARTITION_ID: u16 = 0;

// async fn zenoh_get_json(session: &zenoh::Session, key_expr: &str) -> Value {
//     let key_expr: KeyExpr = key_expr
//         .try_into()
//         .expect("invalid key expression for get query");
//     let replies = session
//         .get(&key_expr)
//         .await
//         .expect("failed to issue get query");
//     let reply = replies.recv_async().await.expect("failed to receive reply");
//     let sample = reply.result().expect("query returned error");
//     let bytes = sample.payload().to_bytes();
//     serde_json::from_slice(bytes.as_ref()).expect("invalid JSON payload")
// }

async fn zenoh_get_json_retry(
    session: &zenoh::Session,
    key_expr: &str,
) -> Value {
    for _ in 0..10 {
        let ke: KeyExpr = key_expr.try_into().unwrap();
        if let Ok(replies) = session.get(&ke).await {
            if let Ok(reply) = replies.recv_async().await {
                if let Ok(sample) = reply.result() {
                    let bytes = sample.payload().to_bytes();
                    if let Ok(v) = serde_json::from_slice(bytes.as_ref()) {
                        return v;
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("failed to retrieve JSON after retries");
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_per_shard_returns_json() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    let odgm = env.start_odgm().await.expect("start odgm");
    env.create_test_collection("caps_coll")
        .await
        .expect("create collection");

    // Find any local shard for this collection
    let shard = odgm
        .get_any_local_shard("caps_coll")
        .await
        .expect("no local shard found for caps_coll");
    let meta = shard.meta().clone();
    let shard_id = meta.id;
    let partition = meta.partition_id as u32;

    let session = env.get_session().await;
    let key = format!(
        "oprc/{}/{}/shards/{}/capabilities",
        "caps_coll", partition, shard_id
    );
    let v = zenoh_get_json_retry(&session, &key).await;

    // Basic shape assertions
    assert_eq!(v["class"], "caps_coll");
    assert_eq!(v["partition"], partition);
    assert_eq!(v["shard_id"], shard_id);
    assert!(v["event_pipeline_v2"].is_boolean());
    assert!(v["storage_backend"].is_string());
    assert!(v["odgm_version"].is_string());
    assert_eq!(v["features"]["granular_storage"], true);
    assert!(v["features"]["bridge_mode"].is_boolean());

    env.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn capabilities_nonexistent_shard_returns_not_found() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    let _odgm = env.start_odgm().await.expect("start odgm");
    env.create_test_collection("caps_coll")
        .await
        .expect("create collection");

    // Choose a definitely non-existent shard id
    let bogus_shard_id = 9_999_999_u64;
    let partition = PARTITION_ID as u32;

    let session = env.get_session().await;
    let key = format!(
        "oprc/{}/{}/shards/{}/capabilities",
        "caps_coll", partition, bogus_shard_id
    );
    let v = zenoh_get_json_retry(&session, &key).await;
    assert_eq!(v["error"], "not_found");

    env.shutdown().await;
}
