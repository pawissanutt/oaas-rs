#![allow(unused_variables)] // Cluster tests are temporarily disabled

mod common;

use common::{TestEnvironment, setup};
use std::time::Duration;

/// Test two-node cluster formation and basic operations
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore] // Disable cluster tests until ready
async fn test_two_node_cluster() {
    let configs = setup::create_cluster_configs(2).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;

    // Start both nodes
    let odgm1 = env1
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 1");
    let odgm2 = env2
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 2");

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify both nodes are aware of each other
    assert_eq!(odgm1.node_id, 1);
    assert_eq!(odgm2.node_id, 2);

    // Create collection on node 1
    env1.create_test_collection("cluster_test")
        .await
        .expect("Failed to create collection on node 1");

    // Wait for replication
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify collection exists on both nodes
    let exists1 = env1
        .collection_exists("cluster_test")
        .await
        .expect("Failed to check collection on node 1");
    let exists2 = env2
        .collection_exists("cluster_test")
        .await
        .expect("Failed to check collection on node 2");

    assert!(exists1, "Collection not found on node 1");
    assert!(exists2, "Collection not found on node 2");

    env1.shutdown().await;
    env2.shutdown().await;
}

/// Test three-node cluster with shard distribution
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore] // Disable cluster tests until ready
async fn test_three_node_cluster() {
    let configs = setup::create_cluster_configs(3).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;
    let env3 = TestEnvironment::new(configs[2].clone()).await;

    // Start all nodes
    let odgm1 = env1
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 1");
    let odgm2 = env2
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 2");
    let odgm3 = env3
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 3");

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Create collection with multiple partitions and replicas
    use oprc_pb::CreateCollectionRequest;
    let collection_req = CreateCollectionRequest {
        name: "distributed_collection".to_string(),
        partition_count: 6, // Will be distributed across nodes
        replica_count: 2,   // Each partition replicated twice
        shard_type: "mst".to_string(),
        shard_assignments: vec![],
        options: std::collections::HashMap::new(),
        invocations: None,
    };

    let result = odgm1
        .metadata_manager
        .create_collection(collection_req)
        .await;
    assert!(result.is_ok(), "Failed to create distributed collection");

    // Wait for shard distribution
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify collection exists on all nodes
    let exists1 = env1
        .collection_exists("distributed_collection")
        .await
        .expect("Failed to check collection on node 1");
    let exists2 = env2
        .collection_exists("distributed_collection")
        .await
        .expect("Failed to check collection on node 2");
    let exists3 = env3
        .collection_exists("distributed_collection")
        .await
        .expect("Failed to check collection on node 3");

    assert!(exists1, "Collection not found on node 1");
    assert!(exists2, "Collection not found on node 2");
    assert!(exists3, "Collection not found on node 3");

    // Verify shard distribution
    let shard_count1 = env1
        .get_shard_count()
        .await
        .expect("Failed to get shard count on node 1");
    let shard_count2 = env2
        .get_shard_count()
        .await
        .expect("Failed to get shard count on node 2");
    let shard_count3 = env3
        .get_shard_count()
        .await
        .expect("Failed to get shard count on node 3");

    println!(
        "Shard distribution - Node 1: {}, Node 2: {}, Node 3: {}",
        shard_count1, shard_count2, shard_count3
    );

    // Each node should have some shards
    assert!(shard_count1 > 0, "Node 1 should have shards");
    assert!(shard_count2 > 0, "Node 2 should have shards");
    assert!(shard_count3 > 0, "Node 3 should have shards");

    // Total shard instances should be 6 partitions * 2 replicas = 12
    let total_shards = shard_count1 + shard_count2 + shard_count3;
    assert_eq!(total_shards, 12, "Expected 12 total shard instances");

    env1.shutdown().await;
    env2.shutdown().await;
    env3.shutdown().await;
}

/// Test node failure and recovery
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore] // Disable cluster tests until ready
async fn test_node_failure_recovery() {
    let configs = setup::create_cluster_configs(3).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;
    let env3 = TestEnvironment::new(configs[2].clone()).await;

    // Start all nodes
    let odgm1 = env1
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 1");
    let odgm2 = env2
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 2");
    let odgm3 = env3
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 3");

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Create collection
    env1.create_test_collection("failure_test")
        .await
        .expect("Failed to create collection");

    // Wait for replication
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify initial state
    let initial_exists1 = env1
        .collection_exists("failure_test")
        .await
        .expect("Failed to check collection on node 1");
    let initial_exists2 = env2
        .collection_exists("failure_test")
        .await
        .expect("Failed to check collection on node 2");
    let initial_exists3 = env3
        .collection_exists("failure_test")
        .await
        .expect("Failed to check collection on node 3");

    assert!(initial_exists1);
    assert!(initial_exists2);
    assert!(initial_exists3);

    // Simulate node 2 failure
    env2.shutdown().await;

    // Wait for failure detection
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify remaining nodes still function
    let post_failure_exists1 = env1
        .collection_exists("failure_test")
        .await
        .expect("Failed to check collection on node 1");
    let post_failure_exists3 = env3
        .collection_exists("failure_test")
        .await
        .expect("Failed to check collection on node 3");

    assert!(
        post_failure_exists1,
        "Node 1 should still have collection after node 2 failure"
    );
    assert!(
        post_failure_exists3,
        "Node 3 should still have collection after node 2 failure"
    );

    // TODO: Test that data operations still work on remaining nodes
    // TODO: Test node recovery when supported

    env1.shutdown().await;
    env3.shutdown().await;
}

/// Test cluster consensus for metadata operations
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore] // Disable cluster tests until ready
async fn test_cluster_consensus() {
    let configs = setup::create_cluster_configs(3).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;
    let env3 = TestEnvironment::new(configs[2].clone()).await;

    // Start all nodes
    let odgm1 = env1
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 1");
    let odgm2 = env2
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 2");
    let odgm3 = env3
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 3");

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Create multiple collections simultaneously from different nodes
    let collection_names =
        vec!["consensus_test_1", "consensus_test_2", "consensus_test_3"];

    // Create collections from different nodes
    env1.create_test_collection(collection_names[0])
        .await
        .expect("Failed to create collection 1");
    env2.create_test_collection(collection_names[1])
        .await
        .expect("Failed to create collection 2");
    env3.create_test_collection(collection_names[2])
        .await
        .expect("Failed to create collection 3");

    // Wait for consensus and replication
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify all collections exist on all nodes
    for collection_name in &collection_names {
        let exists1 = env1
            .collection_exists(collection_name)
            .await
            .expect("Failed to check collection on node 1");
        let exists2 = env2
            .collection_exists(collection_name)
            .await
            .expect("Failed to check collection on node 2");
        let exists3 = env3
            .collection_exists(collection_name)
            .await
            .expect("Failed to check collection on node 3");

        assert!(
            exists1,
            "Collection {} not found on node 1",
            collection_name
        );
        assert!(
            exists2,
            "Collection {} not found on node 2",
            collection_name
        );
        assert!(
            exists3,
            "Collection {} not found on node 3",
            collection_name
        );
    }

    env1.shutdown().await;
    env2.shutdown().await;
    env3.shutdown().await;
}

/// Test load balancing across cluster nodes
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore] // Disable cluster tests until ready
async fn test_cluster_load_balancing() {
    let configs = setup::create_cluster_configs(2).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;

    let odgm1 = env1
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 1");
    let odgm2 = env2
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 2");

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Create collection with multiple partitions
    use oprc_pb::CreateCollectionRequest;
    let collection_req = CreateCollectionRequest {
        name: "load_balance_test".to_string(),
        partition_count: 4,
        replica_count: 1,
        shard_type: "mst".to_string(),
        shard_assignments: vec![],
        options: std::collections::HashMap::new(),
        invocations: None,
    };

    let result = odgm1
        .metadata_manager
        .create_collection(collection_req)
        .await;
    assert!(result.is_ok(), "Failed to create collection");

    // Wait for shard distribution
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify shards are distributed across nodes
    let shard_count1 = env1
        .get_shard_count()
        .await
        .expect("Failed to get shard count on node 1");
    let shard_count2 = env2
        .get_shard_count()
        .await
        .expect("Failed to get shard count on node 2");

    println!(
        "Load balance - Node 1: {} shards, Node 2: {} shards",
        shard_count1, shard_count2
    );

    // Both nodes should have some shards (load balanced)
    assert!(shard_count1 > 0, "Node 1 should have some shards");
    assert!(shard_count2 > 0, "Node 2 should have some shards");
    assert_eq!(shard_count1 + shard_count2, 4, "Total shards should be 4");

    // TODO: Test that data operations are routed to appropriate nodes
    // TODO: Test that load is actually balanced in data operations

    env1.shutdown().await;
    env2.shutdown().await;
}

/// Test cluster network partitions
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore] // Disable cluster tests until ready
async fn test_network_partition() {
    let configs = setup::create_cluster_configs(3).await;
    let env1 = TestEnvironment::new(configs[0].clone()).await;
    let env2 = TestEnvironment::new(configs[1].clone()).await;
    let env3 = TestEnvironment::new(configs[2].clone()).await;

    let odgm1 = env1
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 1");
    let odgm2 = env2
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 2");
    let odgm3 = env3
        .start_odgm()
        .await
        .expect("Failed to start ODGM node 3");

    // Wait for cluster formation
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Create initial collection
    env1.create_test_collection("partition_test")
        .await
        .expect("Failed to create collection");

    // Wait for replication
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify initial state
    let exists1 = env1
        .collection_exists("partition_test")
        .await
        .expect("Failed to check collection on node 1");
    let exists2 = env2
        .collection_exists("partition_test")
        .await
        .expect("Failed to check collection on node 2");
    let exists3 = env3
        .collection_exists("partition_test")
        .await
        .expect("Failed to check collection on node 3");

    assert!(exists1);
    assert!(exists2);
    assert!(exists3);

    // Simulate network partition by shutting down node 1
    // (In a real test, we would simulate network issues rather than shutdown)
    env1.shutdown().await;

    // Wait for partition detection
    tokio::time::sleep(Duration::from_millis(2000)).await;

    // Verify that majority partition (nodes 2 and 3) continues to function
    let post_partition_exists2 = env2
        .collection_exists("partition_test")
        .await
        .expect("Failed to check collection on node 2");
    let post_partition_exists3 = env3
        .collection_exists("partition_test")
        .await
        .expect("Failed to check collection on node 3");

    assert!(
        post_partition_exists2,
        "Node 2 should maintain collection during partition"
    );
    assert!(
        post_partition_exists3,
        "Node 3 should maintain collection during partition"
    );

    // TODO: Test that minority partition cannot make changes
    // TODO: Test partition healing when node comes back

    env2.shutdown().await;
    env3.shutdown().await;
}
