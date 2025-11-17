mod common;

use common::{TestEnvironment, setup};
use std::time::Duration;

use oprc_grpc::{CreateCollectionRequest, ShardAssignment, ValData, ValType};
use oprc_odgm::shard::{ObjectData, ObjectVal};

// Raft cluster replication across 3 nodes with 1 partition, 3 replicas
// Notes:
// - No cluster-wide metadata sync: we create the same collection on each node
// - We set `raft_init_leader_only=true` so only the primary initializes the Raft cluster
// - We choose shard assignments so partition 0 has shard_ids [1,2,3] with replica owners [1,2,3]
// - We write on partition 0 and assert replication on all nodes via local shard
#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_raft_three_node_replication() {
    let configs = setup::create_cluster_configs(3).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;
    let env3 = TestEnvironment::new(configs[2].clone()).await;

    // Start nodes
    let odgm1 = env1.start_odgm().await.expect("start node1");
    let odgm2 = env2.start_odgm().await.expect("start node2");
    let odgm3 = env3.start_odgm().await.expect("start node3");

    // Wait for cluster bring-up
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // Build explicit shard assignment for Raft: 1 partition, 3 replicas across nodes 1,2,3
    let mut options = std::collections::HashMap::new();
    options.insert("raft_init_leader_only".to_string(), "true".to_string());

    let assignments = vec![ShardAssignment {
        primary: Some(1),
        replica: vec![1, 2, 3],
        shard_ids: vec![1, 2, 3],
    }];

    let req = CreateCollectionRequest {
        name: "raft_repl".to_string(),
        partition_count: 1,
        replica_count: 3,
        shard_type: "raft".to_string(),
        shard_assignments: assignments.clone(),
        options: options.clone(),
        invocations: None,
    };

    // Create the same collection on each node (no metadata sync)
    assert!(
        odgm1
            .metadata_manager
            .create_collection(req.clone())
            .await
            .is_ok()
    );
    assert!(
        odgm2
            .metadata_manager
            .create_collection(req.clone())
            .await
            .is_ok()
    );
    assert!(
        odgm3
            .metadata_manager
            .create_collection(req.clone())
            .await
            .is_ok()
    );

    // Allow Raft networking and election to settle
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Write on node 1 (any replica can propose; leader will handle)
    let shard1 = odgm1
        .get_local_shard("raft_repl", 0)
        .await
        .expect("node1 missing local shard");

    let mut obj = ObjectData::new();
    obj.value.insert(
        7u32,
        ObjectVal::from(ValData {
            r#type: ValType::Byte as i32,
            data: b"rv1".to_vec(),
        }),
    );

    shard1
        .set_object(1001, obj)
        .await
        .expect("set_object on node1");

    // Poll followers for replicated value
    let shard2 = odgm2
        .get_local_shard("raft_repl", 0)
        .await
        .expect("node2 missing local shard");
    let shard3 = odgm3
        .get_local_shard("raft_repl", 0)
        .await
        .expect("node3 missing local shard");

    let mut ok2 = false;
    let mut ok3 = false;
    for _ in 0..30 {
        if !ok2 {
            if let Some(e) = shard2.get_object(1001).await.expect("get node2") {
                if e.value.get(&7u32).map(|v| &v.data) == Some(&b"rv1".to_vec())
                {
                    ok2 = true;
                }
            }
        }
        if !ok3 {
            if let Some(e) = shard3.get_object(1001).await.expect("get node3") {
                if e.value.get(&7u32).map(|v| &v.data) == Some(&b"rv1".to_vec())
                {
                    ok3 = true;
                }
            }
        }
        if ok2 && ok3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert!(ok2, "Node 2 did not replicate value in time");
    assert!(ok3, "Node 3 did not replicate value in time");

    env1.shutdown().await;
    env2.shutdown().await;
    env3.shutdown().await;
}

// Leader failover: kill node1, ensure re-election and subsequent writes succeed
#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_raft_leader_failover_write() {
    let configs = setup::create_cluster_configs(3).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;
    let env3 = TestEnvironment::new(configs[2].clone()).await;

    let odgm1 = env1.start_odgm().await.expect("start node1");
    let odgm2 = env2.start_odgm().await.expect("start node2");
    let odgm3 = env3.start_odgm().await.expect("start node3");

    tokio::time::sleep(Duration::from_millis(1200)).await;

    let mut options = std::collections::HashMap::new();
    options.insert("raft_init_leader_only".to_string(), "true".to_string());

    let assignments = vec![ShardAssignment {
        primary: Some(1),
        replica: vec![1, 2, 3],
        shard_ids: vec![10, 20, 30],
    }];

    let req = CreateCollectionRequest {
        name: "raft_failover".to_string(),
        partition_count: 1,
        replica_count: 3,
        shard_type: "raft".to_string(),
        shard_assignments: assignments,
        options,
        invocations: None,
    };

    assert!(
        odgm1
            .metadata_manager
            .create_collection(req.clone())
            .await
            .is_ok()
    );
    assert!(
        odgm2
            .metadata_manager
            .create_collection(req.clone())
            .await
            .is_ok()
    );
    assert!(odgm3.metadata_manager.create_collection(req).await.is_ok());

    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Write initial value
    let shard1 = odgm1
        .get_local_shard("raft_failover", 0)
        .await
        .expect("node1 shard");
    let mut obj = ObjectData::new();
    obj.value.insert(
        9u32,
        ObjectVal::from(ValData {
            r#type: ValType::Byte as i32,
            data: b"a".to_vec(),
        }),
    );
    shard1
        .set_object(2002, obj)
        .await
        .expect("write before failover");

    // Ensure replicated once before failover
    let shard2 = odgm2
        .get_local_shard("raft_failover", 0)
        .await
        .expect("node2 shard");
    for _ in 0..20 {
        if let Some(e) = shard2.get_object(2002).await.expect("get n2") {
            if e.value.get(&9u32).map(|v| &v.data) == Some(&b"a".to_vec()) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Simulate leader crash
    env1.shutdown().await;

    // Wait for re-election; election timeout is randomized, so give it time
    tokio::time::sleep(Duration::from_millis(3000)).await;

    // Write from node2 (should succeed with new leader)
    let shard2 = odgm2
        .get_local_shard("raft_failover", 0)
        .await
        .expect("node2 shard");
    let mut obj2 = ObjectData::new();
    obj2.value.insert(
        9u32,
        ObjectVal::from(ValData {
            r#type: ValType::Byte as i32,
            data: b"b".to_vec(),
        }),
    );
    // Write from node2 (should succeed with new leader), retry until a new leader is elected
    // OpenRaft may still report the old leader briefly; tolerate transient NotLeader/Network errors.
    let mut wrote = false;
    for _ in 0..50 {
        // ~10s max with 200ms sleep
        match shard2.set_object(2002, obj2.clone()).await {
            Ok(()) => {
                wrote = true;
                break;
            }
            Err(e) => {
                // Allow transient routing/election errors and retry
                let msg = format!("{}", e);
                if msg.contains("Not leader")
                    || msg.contains("Failed to forward")
                    || msg.contains("No quaryable target")
                    || msg.contains("sender dropped")
                    || msg.contains("session closed")
                {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                } else {
                    panic!("unexpected error on post-failover write: {}", msg);
                }
            }
        }
    }
    assert!(wrote, "write after failover did not succeed in time");

    // Verify node3 has updated value
    let shard3 = odgm3
        .get_local_shard("raft_failover", 0)
        .await
        .expect("node3 shard");
    let mut ok = false;
    for _ in 0..30 {
        if let Some(e) = shard3.get_object(2002).await.expect("get n3") {
            if e.value.get(&9u32).map(|v| &v.data) == Some(&b"b".to_vec()) {
                ok = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(ok, "node3 did not reflect post-failover write");

    env2.shutdown().await;
    env3.shutdown().await;
}
