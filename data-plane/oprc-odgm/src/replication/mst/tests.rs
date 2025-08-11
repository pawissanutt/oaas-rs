//! Tests for MST replication layer

#[cfg(test)]
mod tests {
    use super::super::{mst_layer::MstReplicationLayer, types::MstConfig};
    use crate::replication::{
        Operation, ReadOperation, ReplicationLayer, ReplicationModel,
        ResponseStatus, ShardRequest, WriteOperation,
    };
    use crate::shard::ShardMetadata;
    use oprc_dp_storage::{MemoryStorage, StorageValue};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Hash)]
    struct TestValue {
        data: String,
        timestamp: u64,
    }

    impl TestValue {
        fn new(data: String) -> Self {
            Self {
                data,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
            }
        }
    }

    /// Helper function to create a test Zenoh session
    async fn create_test_zenoh_session() -> zenoh::Session {
        // Create a minimal Zenoh config for testing
        let mut config = zenoh::Config::default();
        config.insert_json5("mode", "\"peer\"").unwrap();
        config.insert_json5("connect/endpoints", "[]").unwrap();
        config.insert_json5("listen/endpoints", "[]").unwrap();
        config
            .insert_json5("scouting/multicast/enabled", "false")
            .unwrap();

        zenoh::open(config).await.unwrap()
    }

    /// Helper function to create proper test metadata with valid collection and partition_id
    fn create_test_metadata() -> ShardMetadata {
        ShardMetadata {
            id: 1,
            collection: "test".to_string(),
            partition_id: 1,
            ..Default::default()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_basic_operations() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Test set and get without initializing (to avoid Zenoh networking issues in tests)
        let value = TestValue::new("test_value".to_string());

        mst_layer.set(123, value.clone()).await.unwrap();
        let retrieved = mst_layer.get(123).await.unwrap().unwrap();

        assert_eq!(retrieved.data, value.data);

        // Test root hash calculation
        let root_hash = mst_layer.get_root_hash().await;
        assert!(root_hash.is_some());

        // Test trigger sync (should not fail with test networking)
        mst_layer.trigger_sync().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_conflict_resolution() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Create two values with different timestamps
        let older_value = TestValue {
            data: "older_value".to_string(),
            timestamp: 1000,
        };

        let newer_value = TestValue {
            data: "newer_value".to_string(),
            timestamp: 2000,
        };

        // Set older value first
        mst_layer.set(456, older_value.clone()).await.unwrap();
        let retrieved1 = mst_layer.get(456).await.unwrap().unwrap();
        assert_eq!(retrieved1.data, older_value.data);

        // Set newer value - should overwrite due to LWW
        mst_layer.set(456, newer_value.clone()).await.unwrap();
        let retrieved2 = mst_layer.get(456).await.unwrap().unwrap();
        assert_eq!(retrieved2.data, newer_value.data);
        assert_eq!(retrieved2.timestamp, newer_value.timestamp);

        // Set older value again - should be ignored due to LWW
        mst_layer.set(456, older_value.clone()).await.unwrap();
        let retrieved3 = mst_layer.get(456).await.unwrap().unwrap();
        assert_eq!(retrieved3.data, newer_value.data); // Still newer value
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_multiple_keys() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Insert multiple keys
        let keys_values = vec![
            (100, TestValue::new("value_100".to_string())),
            (200, TestValue::new("value_200".to_string())),
            (300, TestValue::new("value_300".to_string())),
            (400, TestValue::new("value_400".to_string())),
        ];

        for (key, value) in &keys_values {
            mst_layer.set(*key, value.clone()).await.unwrap();
        }

        // Verify all keys can be retrieved
        for (key, expected_value) in &keys_values {
            let retrieved = mst_layer.get(*key).await.unwrap().unwrap();
            assert_eq!(retrieved.data, expected_value.data);
        }

        // Verify non-existent key returns None
        let non_existent = mst_layer.get(999).await.unwrap();
        assert!(non_existent.is_none());

        // Test root hash changes with data
        let root_hash1 = mst_layer.get_root_hash().await;
        assert!(root_hash1.is_some());

        // Add another key and verify hash changes
        mst_layer
            .set(500, TestValue::new("value_500".to_string()))
            .await
            .unwrap();
        let root_hash2 = mst_layer.get_root_hash().await;
        assert!(root_hash2.is_some());
        assert_ne!(root_hash1, root_hash2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_delete_operations() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Set a value
        let value = TestValue::new("to_be_deleted".to_string());
        mst_layer.set(789, value.clone()).await.unwrap();

        // Verify it exists
        let retrieved = mst_layer.get(789).await.unwrap();
        assert!(retrieved.is_some());

        // Delete it
        mst_layer.delete(789).await.unwrap();

        // Verify it's gone from storage
        let after_delete = mst_layer.get(789).await.unwrap();
        assert!(after_delete.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_replication_layer_trait() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Test replication model
        let model = mst_layer.replication_model();
        assert!(matches!(model, ReplicationModel::ConflictFree { .. }));

        // Test write operation through ReplicationLayer trait
        let write_op = WriteOperation {
            key: StorageValue::from(555u64.to_be_bytes().to_vec()),
            value: StorageValue::from(
                serde_json::to_vec(&TestValue::new(
                    "replication_test".to_string(),
                ))
                .unwrap(),
            ),
            ..Default::default()
        };

        let request = ShardRequest {
            operation: Operation::Write(write_op),
            timestamp: std::time::SystemTime::now(),
            source_node: 1,
        };

        let response = mst_layer.replicate_write(request).await.unwrap();
        assert!(matches!(response.status, ResponseStatus::Applied));

        // Test read operation through ReplicationLayer trait
        let read_op = ReadOperation {
            key: StorageValue::from(555u64.to_be_bytes().to_vec()),
        };

        let read_request = ShardRequest {
            operation: Operation::Read(read_op),
            timestamp: std::time::SystemTime::now(),
            source_node: 1,
        };

        let read_response =
            mst_layer.replicate_read(read_request).await.unwrap();
        assert!(matches!(read_response.status, ResponseStatus::Applied));
        assert!(read_response.data.is_some());

        // Test replication status
        let status = mst_layer.get_replication_status().await.unwrap();
        assert_eq!(status.healthy_replicas, 1);
        assert!(!status.is_leader); // MST doesn't have leaders
        assert_eq!(status.conflicts, 0);

        // Test sync replicas
        mst_layer.sync_replicas().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_add_remove_replicas() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Test adding replicas (should succeed for MST but is mainly informational)
        mst_layer
            .add_replica(2, "node2:8080".to_string())
            .await
            .unwrap();
        mst_layer
            .add_replica(3, "node3:8080".to_string())
            .await
            .unwrap();

        // Test removing replicas
        mst_layer.remove_replica(2).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_readiness_watch() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Test readiness watch
        let readiness_watch = mst_layer.watch_readiness();

        // Should initially be false (not initialized)
        assert_eq!(*readiness_watch.borrow(), false);

        // Signal readiness for test (without starting networking)
        mst_layer.signal_readiness_for_test();

        // Wait a moment for the signal to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let is_ready = *readiness_watch.borrow();
        assert!(is_ready);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_storage_rebuild() {
        let storage = MemoryStorage::default();
        let metadata = create_test_metadata();
        let zenoh_session = create_test_zenoh_session().await;

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer = MstReplicationLayer::new(
            storage,
            1,
            metadata,
            config,
            zenoh_session,
        );

        // Add some data
        let test_data = vec![
            (1001, TestValue::new("data_1".to_string())),
            (1002, TestValue::new("data_2".to_string())),
            (1003, TestValue::new("data_3".to_string())),
        ];

        for (key, value) in &test_data {
            mst_layer.set(*key, value.clone()).await.unwrap();
        }

        // Get root hash before rebuild
        let hash_before = mst_layer.get_root_hash().await;

        // Rebuild MST from storage (simulates restart)
        mst_layer.rebuild_mst_from_storage().await.unwrap();

        // Get root hash after rebuild - should be the same
        let hash_after = mst_layer.get_root_hash().await;
        assert_eq!(hash_before, hash_after);

        // Verify all data is still accessible
        for (key, expected_value) in &test_data {
            let retrieved = mst_layer.get(*key).await.unwrap().unwrap();
            assert_eq!(retrieved.data, expected_value.data);
        }
    }
}
