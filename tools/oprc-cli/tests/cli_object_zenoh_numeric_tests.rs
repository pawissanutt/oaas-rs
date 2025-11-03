use assert_cmd::prelude::*;
use oprc_grpc::CreateCollectionRequest;
use oprc_odgm::metadata::OprcMetaManager;
use oprc_odgm::shard::{
    UnifiedShardConfig, UnifiedShardFactory, UnifiedShardManager,
};
use oprc_odgm::{ObjectDataGridManager, OdgmConfig};
use oprc_zenoh::OprcZenohConfig;
use oprc_zenoh::pool::Pool;
use std::process::Command;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use zenoh_config::WhatAmI;

// Pick a free TCP port by binding to port 0 then releasing it
fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

async fn start_odgm_with_zenoh(zenoh_port: u16) -> String {
    // Configure ODGM and build a Zenoh pool explicitly (no env)
    let mut cfg = OdgmConfig::default();
    cfg.node_id = Some(1);
    cfg.members = Some("1".into());
    cfg.events_enabled = false;

    // Build explicit zenoh config
    let z_conf = OprcZenohConfig {
        zenoh_port,
        mode: WhatAmI::Peer,
        gossip_enabled: Some(true),
        ..Default::default()
    };
    let session_pool = Pool::new(cfg.max_sessions as usize, z_conf);

    // Metadata & shard managers
    let node_id = cfg.node_id.unwrap();
    let members = vec![node_id];
    let metadata_manager = Arc::new(OprcMetaManager::new(node_id, members));
    let factory_config = UnifiedShardConfig {
        enable_string_ids: cfg.enable_string_ids,
        max_string_id_len: cfg.max_string_id_len,
        granular_prefetch_limit: cfg.granular_prefetch_limit,
    };
    let shard_factory = Arc::new(UnifiedShardFactory::new(
        session_pool.clone(),
        factory_config,
    ));
    let shard_manager = Arc::new(UnifiedShardManager::new(shard_factory));
    let odgm = ObjectDataGridManager::new(
        node_id,
        metadata_manager.clone(),
        shard_manager.clone(),
    )
    .await;
    odgm.start_watch_stream();

    // Create a test collection
    let collection = format!("cli_zenoh_numeric_{}", nanoid::nanoid!(6));
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

    // Wait for shard creation and readiness to avoid Zenoh Disconnected races
    // 1) Wait until at least one shard is created
    let mut attempts = 0;
    while attempts < 100 {
        let stats = shard_manager.get_stats().await;
        if stats.total_shards_created >= 1 {
            break;
        }
        sleep(Duration::from_millis(50)).await;
        attempts += 1;
    }

    // 2) Wait until all shards for this collection report ready (replication ready implies network started)
    let mut attempts = 0;
    while attempts < 100 {
        let shards = shard_manager.get_shards_for_collection(&collection).await;
        if !shards.is_empty() {
            let all_ready =
                shards.iter().all(|s| *s.watch_readiness().borrow());
            if all_ready {
                break;
            }
        }
        sleep(Duration::from_millis(50)).await;
        attempts += 1;
    }

    collection
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_object_set_get_via_zenoh_numeric_id() {
    let zenoh_port = pick_free_port();
    let collection = start_odgm_with_zenoh(zenoh_port).await;
    let peer = format!("tcp/127.0.0.1:{}", zenoh_port);

    // Set object with numeric id and a numeric entry
    Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"))
        .args([
            "object",
            "set",
            "--cls-id",
            &collection,
            "0",
            "777",
            "-b",
            "1=hello",
            "-z",
            &peer,
        ])
        .assert()
        .success();

    // Get the specific entry via zenoh
    Command::new(assert_cmd::cargo::cargo_bin!("oprc-cli"))
        .args([
            "object",
            "get",
            "--cls-id",
            &collection,
            "0",
            "777",
            "--key",
            "1",
            "-z",
            &peer,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("hello"));
}
