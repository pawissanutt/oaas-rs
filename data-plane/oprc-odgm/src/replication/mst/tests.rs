//! Tests for MST replication layer

#[cfg(test)]
mod tests {
    use super::super::{
        error::MstError,
        layer::MstReplicationLayer,
        networking::MockMstNetworking,
        traits::{MstNetworking, MstPageRequestHandler, MstPageUpdateHandler},
        types::{
            GenericLoadPageReq, GenericNetworkPage, GenericPagesResp, MstConfig,
        },
    };
    use crate::replication::{
        Operation, ReadOperation, ReplicationLayer, ReplicationModel,
        ResponseStatus, ShardRequest, WriteOperation,
    };
    use crate::shard::ShardMetadata;
    use async_trait::async_trait;
    use oprc_dp_storage::{MemoryStorage, StorageValue};
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_basic_operations() {
        let storage = MemoryStorage::default();
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

        // Test set and get without initializing (to avoid Zenoh issues in tests)
        let value = TestValue::new("test_value".to_string());

        mst_layer.set(123, value.clone()).await.unwrap();
        let retrieved = mst_layer.get(123).await.unwrap().unwrap();

        assert_eq!(retrieved.data, value.data);

        // Test root hash calculation
        let root_hash = mst_layer.get_root_hash().await;
        assert!(root_hash.is_some());

        // Test trigger sync (should not fail with mock networking)
        mst_layer.trigger_sync().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_conflict_resolution() {
        let storage = MemoryStorage::default();
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

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
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

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
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

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
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

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
            ttl: None,
        };

        let request = ShardRequest {
            operation: Operation::Write(write_op),
            timestamp: std::time::SystemTime::now(),
            source_node: 1,
            request_id: "test_request_1".to_string(),
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
            request_id: "test_request_2".to_string(),
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
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

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
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

        // Test readiness watch
        let readiness_watch = mst_layer.watch_readiness();

        // Should initially be false (not initialized)
        assert_eq!(*readiness_watch.borrow(), false);

        // Initialize should set readiness to true
        mst_layer.initialize().await.unwrap();

        // Wait for readiness change
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let is_ready = *readiness_watch.borrow();
        assert!(is_ready);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_storage_rebuild() {
        let storage = MemoryStorage::default();
        let metadata = ShardMetadata::default();
        let networking = MockMstNetworking::new();

        let config = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let mst_layer =
            MstReplicationLayer::new(storage, 1, metadata, config, networking);

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

    /// Networked MST implementation for multi-node testing
    pub struct NetworkedMstTesting<T> {
        node_id: u64,
        other_nodes: Arc<RwLock<Vec<Arc<NetworkedMstTesting<T>>>>>,
        page_request_handler: Arc<
            RwLock<Option<Arc<dyn MstPageRequestHandler<T> + Send + Sync>>>,
        >,
        page_update_handler:
            Arc<RwLock<Option<Arc<dyn MstPageUpdateHandler<T> + Send + Sync>>>>,
        published_pages: Arc<RwLock<Vec<(u64, Vec<GenericNetworkPage>)>>>,
    }

    impl<T> NetworkedMstTesting<T> {
        pub fn new(node_id: u64) -> Self {
            Self {
                node_id,
                other_nodes: Arc::new(RwLock::new(Vec::new())),
                page_request_handler: Arc::new(RwLock::new(None)),
                page_update_handler: Arc::new(RwLock::new(None)),
                published_pages: Arc::new(RwLock::new(Vec::new())),
            }
        }

        pub async fn connect_to(&self, other: Arc<NetworkedMstTesting<T>>) {
            self.other_nodes.write().await.push(other.clone());
            other.other_nodes.write().await.push(Arc::new(Self {
                node_id: self.node_id,
                other_nodes: self.other_nodes.clone(),
                page_request_handler: self.page_request_handler.clone(),
                page_update_handler: self.page_update_handler.clone(),
                published_pages: self.published_pages.clone(),
            }));
        }

        pub async fn get_published_pages(
            &self,
        ) -> Vec<(u64, Vec<GenericNetworkPage>)> {
            self.published_pages.read().await.clone()
        }

        pub async fn simulate_network_delivery(&self) {
            let pages = self.published_pages.read().await.clone();
            let other_nodes = self.other_nodes.read().await.clone();

            for (_owner, _page_list) in pages {
                for _node in &other_nodes {
                    // TODO: Implement proper message delivery simulation
                }
            }
        }
    }

    #[async_trait]
    impl<T> MstNetworking<T> for NetworkedMstTesting<T>
    where
        T: Clone
            + Send
            + Sync
            + Serialize
            + for<'de> Deserialize<'de>
            + 'static,
    {
        type Error = MstError;

        async fn start(&self) -> Result<(), Self::Error> {
            // Simulate network start
            Ok(())
        }

        async fn stop(&self) -> Result<(), Self::Error> {
            // Simulate network stop
            Ok(())
        }

        async fn publish_pages(
            &self,
            owner: u64,
            pages: Vec<GenericNetworkPage>,
        ) -> Result<(), Self::Error> {
            // Store published pages for simulation
            self.published_pages.write().await.push((owner, pages));
            Ok(())
        }

        async fn request_pages(
            &self,
            peer: u64,
            _request: GenericLoadPageReq,
        ) -> Result<GenericPagesResp<T>, Self::Error> {
            // Find the peer and request pages from it
            let other_nodes = self.other_nodes.read().await.clone();

            for node in &other_nodes {
                if node.node_id == peer {
                    // TODO: Implement proper page request handling
                }
            }

            // If peer not found, return empty response
            Ok(GenericPagesResp {
                items: BTreeMap::new(),
            })
        }

        fn set_page_request_handler(
            &self,
            handler: Arc<dyn MstPageRequestHandler<T> + Send + Sync>,
        ) {
            tokio::spawn({
                let handler_ref = self.page_request_handler.clone();
                async move {
                    *handler_ref.write().await = Some(handler);
                }
            });
        }

        fn set_page_update_handler(
            &self,
            handler: Arc<dyn MstPageUpdateHandler<T> + Send + Sync>,
        ) {
            tokio::spawn({
                let handler_ref = self.page_update_handler.clone();
                async move {
                    *handler_ref.write().await = Some(handler);
                }
            });
        }
    }

    // Also implement for Arc<NetworkedMstTesting<T>> to work with our test setup
    #[async_trait]
    impl<T> MstNetworking<T> for Arc<NetworkedMstTesting<T>>
    where
        T: Clone
            + Send
            + Sync
            + Serialize
            + for<'de> Deserialize<'de>
            + 'static,
    {
        type Error = MstError;

        async fn start(&self) -> Result<(), Self::Error> {
            self.as_ref().start().await
        }

        async fn stop(&self) -> Result<(), Self::Error> {
            self.as_ref().stop().await
        }

        async fn publish_pages(
            &self,
            owner: u64,
            pages: Vec<GenericNetworkPage>,
        ) -> Result<(), Self::Error> {
            self.as_ref().publish_pages(owner, pages).await
        }

        async fn request_pages(
            &self,
            peer: u64,
            request: GenericLoadPageReq,
        ) -> Result<GenericPagesResp<T>, Self::Error> {
            self.as_ref().request_pages(peer, request).await
        }

        fn set_page_request_handler(
            &self,
            handler: Arc<dyn MstPageRequestHandler<T> + Send + Sync>,
        ) {
            self.as_ref().set_page_request_handler(handler)
        }

        fn set_page_update_handler(
            &self,
            handler: Arc<dyn MstPageUpdateHandler<T> + Send + Sync>,
        ) {
            self.as_ref().set_page_update_handler(handler)
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_multi_node_replication() {
        // Create three nodes
        let storage1 = MemoryStorage::default();
        let storage2 = MemoryStorage::default();
        let storage3 = MemoryStorage::default();

        let metadata1 = ShardMetadata::default();
        let metadata2 = ShardMetadata::default();
        let metadata3 = ShardMetadata::default();

        let networking1 = Arc::new(NetworkedMstTesting::new(1));
        let networking2 = Arc::new(NetworkedMstTesting::new(2));
        let networking3 = Arc::new(NetworkedMstTesting::new(3));

        // Connect nodes to each other
        networking1.connect_to(networking2.clone()).await;
        networking1.connect_to(networking3.clone()).await;
        networking2.connect_to(networking3.clone()).await;

        let config1 = MstConfig::simple_lww(|v: &TestValue| v.timestamp);
        let config2 = MstConfig::simple_lww(|v: &TestValue| v.timestamp);
        let config3 = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let node1 = MstReplicationLayer::new(
            storage1,
            1,
            metadata1,
            config1,
            networking1.clone(),
        );
        let node2 = MstReplicationLayer::new(
            storage2,
            2,
            metadata2,
            config2,
            networking2.clone(),
        );
        let node3 = MstReplicationLayer::new(
            storage3,
            3,
            metadata3,
            config3,
            networking3.clone(),
        );

        // Initialize all nodes
        node1.initialize().await.unwrap();
        node2.initialize().await.unwrap();
        node3.initialize().await.unwrap();

        // Add different data to each node
        let value1 = TestValue {
            data: "node1_data".to_string(),
            timestamp: 1000,
        };
        let value2 = TestValue {
            data: "node2_data".to_string(),
            timestamp: 1100,
        };
        let value3 = TestValue {
            data: "node3_data".to_string(),
            timestamp: 1200,
        };

        node1.set(100, value1.clone()).await.unwrap();
        node2.set(200, value2.clone()).await.unwrap();
        node3.set(300, value3.clone()).await.unwrap();

        // Trigger sync on all nodes to publish their MST pages
        node1.trigger_sync().await.unwrap();
        node2.trigger_sync().await.unwrap();
        node3.trigger_sync().await.unwrap();

        // Simulate network message delivery
        networking1.simulate_network_delivery().await;
        networking2.simulate_network_delivery().await;
        networking3.simulate_network_delivery().await;

        // Allow time for async operations
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify that each node eventually has all data (eventual consistency)
        // Note: In a real implementation, this would require multiple rounds of sync
        // For now, we test that the infrastructure is set up correctly

        // Verify each node has its own data
        assert_eq!(node1.get(100).await.unwrap().unwrap().data, value1.data);
        assert_eq!(node2.get(200).await.unwrap().unwrap().data, value2.data);
        assert_eq!(node3.get(300).await.unwrap().unwrap().data, value3.data);

        // Verify that published pages were created
        let pages1 = networking1.get_published_pages().await;
        let pages2 = networking2.get_published_pages().await;
        let pages3 = networking3.get_published_pages().await;

        assert!(!pages1.is_empty());
        assert!(!pages2.is_empty());
        assert!(!pages3.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_mst_conflict_resolution_across_nodes() {
        // Create two nodes
        let storage1 = MemoryStorage::default();
        let storage2 = MemoryStorage::default();

        let metadata1 = ShardMetadata::default();
        let metadata2 = ShardMetadata::default();

        let networking1 = Arc::new(NetworkedMstTesting::new(1));
        let networking2 = Arc::new(NetworkedMstTesting::new(2));

        // Connect nodes
        networking1.connect_to(networking2.clone()).await;

        let config1 = MstConfig::simple_lww(|v: &TestValue| v.timestamp);
        let config2 = MstConfig::simple_lww(|v: &TestValue| v.timestamp);

        let node1 = MstReplicationLayer::new(
            storage1,
            1,
            metadata1,
            config1,
            networking1.clone(),
        );
        let node2 = MstReplicationLayer::new(
            storage2,
            2,
            metadata2,
            config2,
            networking2.clone(),
        );

        // Initialize nodes
        node1.initialize().await.unwrap();
        node2.initialize().await.unwrap();

        // Test conflict resolution would be implemented here
        // For now, just test basic setup
        let value1 = TestValue {
            data: "node1_value".to_string(),
            timestamp: 1000,
        };

        node1.set(100, value1.clone()).await.unwrap();
        let retrieved = node1.get(100).await.unwrap().unwrap();
        assert_eq!(retrieved.data, value1.data);
    }
}
