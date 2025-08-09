use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use oprc_odgm::{ObjectDataGridManager, OdgmConfig};
use oprc_pb::CreateCollectionRequest;
use oprc_zenoh::pool::Pool;

#[cfg(test)]
pub mod mock_fn;

#[derive(Clone)]
pub struct TestConfig {
    pub odgm_config: OdgmConfig,
    pub _grpc_port: u16, // Keep for potential future use
}

impl TestConfig {
    pub async fn new() -> Self {
        Self {
            odgm_config: OdgmConfig {
                http_port: Self::find_free_port().await,
                node_id: Some(rand::random()),
                members: None, // Will be set based on node_id
                max_sessions: 1,
                reflection_enabled: false,
                events_enabled: true,
                max_trigger_depth: 10,
                trigger_timeout_ms: 30000,
                collection: None,
                node_addr: None,
            },
            _grpc_port: Self::find_free_port().await,
        }
    }

    #[allow(dead_code)] // Will be used when cluster tests are enabled
    pub async fn with_node_id(node_id: u64) -> Self {
        let mut config = Self::new().await;
        config.odgm_config.node_id = Some(node_id);
        config.odgm_config.members = Some(node_id.to_string());
        config
    }

    async fn find_free_port() -> u16 {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to random port");
        let addr = listener.local_addr().expect("Failed to get local addr");
        addr.port()
    }
}

/// Test environment that manages ODGM instances and gRPC servers
#[derive(Clone)]
pub struct TestEnvironment {
    pub config: TestConfig,
    odgm: Arc<RwLock<Option<Arc<ObjectDataGridManager>>>>,
    session_pool: Arc<RwLock<Option<Pool>>>,
    grpc_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl TestEnvironment {
    pub async fn new(config: TestConfig) -> Self {
        // Set up members list for the config
        let mut final_config = config.clone();
        if final_config.odgm_config.members.is_none() {
            if let Some(node_id) = final_config.odgm_config.node_id {
                final_config.odgm_config.members = Some(node_id.to_string());
            }
        }

        Self {
            config: final_config,
            odgm: Arc::new(RwLock::new(None)),
            session_pool: Arc::new(RwLock::new(None)),
            grpc_handle: Arc::new(RwLock::new(None)),
        }
    }

    #[allow(dead_code)]
    pub async fn get_session(&self) -> zenoh::Session {
        let pool = self.session_pool.read().await;
        let pool = pool.as_ref().unwrap();
        let session = pool
            .get_session()
            .await
            .expect("Failed to get Zenoh session");
        session
    }

    /// Start ODGM server and return the manager
    pub async fn start_odgm(
        &self,
    ) -> Result<Arc<ObjectDataGridManager>, Box<dyn std::error::Error>> {
        // Use start_server to launch ODGM with gRPC server
        let (odgm_arc, pool) =
            oprc_odgm::start_server(&self.config.odgm_config).await?;

        // Store references
        *self.odgm.write().await = Some(odgm_arc.clone());
        // session_pool is not available from start_server, so leave as None
        *self.session_pool.write().await = Some(pool);

        if let Some(_) = self.config.odgm_config.collection {
            oprc_odgm::create_collection(
                odgm_arc.clone(),
                &self.config.odgm_config,
            )
            .await;
        }

        Ok(odgm_arc)
    }

    /// Create a test collection
    #[allow(dead_code)] // Used by integration tests via env.create_test_collection
    pub async fn create_test_collection(
        &self,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let odgm_guard = self.odgm.read().await;
        let odgm = odgm_guard.as_ref().ok_or("ODGM not started")?;

        let collection_req = CreateCollectionRequest {
            name: name.to_string(),
            partition_count: 2,
            replica_count: 1,
            shard_type: "mst".to_string(),
            shard_assignments: vec![],
            options: std::collections::HashMap::new(),
            invocations: None,
        };

        odgm.metadata_manager
            .create_collection(collection_req)
            .await?;

        // Wait for shards to be created
        tokio::time::sleep(Duration::from_millis(1000)).await;

        Ok(())
    }

    #[allow(dead_code)] // Used by integration tests via env.collection_exists
    /// Check if collection exists
    pub async fn collection_exists(
        &self,
        name: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let odgm_guard = self.odgm.read().await;
        let odgm = odgm_guard.as_ref().ok_or("ODGM not started")?;

        let collections = odgm.metadata_manager.collections.read().await;
        Ok(collections.contains_key(name))
    }

    #[allow(dead_code)] // Used by integration tests via env.get_shard_count
    /// Get shard count
    pub async fn get_shard_count(
        &self,
    ) -> Result<u32, Box<dyn std::error::Error>> {
        let odgm_guard = self.odgm.read().await;
        let odgm = odgm_guard.as_ref().ok_or("ODGM not started")?;

        Ok(odgm.shard_manager.get_stats().await.total_shards_created)
    }

    /// Shutdown all components
    pub async fn shutdown(&self) {
        // Shutdown gRPC server
        if let Some(handle) = self.grpc_handle.write().await.take() {
            handle.abort();
        }

        // Shutdown ODGM
        if let Some(odgm) = self.odgm.write().await.take() {
            odgm.close().await;
        }

        // Clear session pool
        *self.session_pool.write().await = None;
    }
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // Ensure cleanup happens
        let odgm = self.odgm.clone();
        let grpc_handle = self.grpc_handle.clone();

        tokio::spawn(async move {
            if let Some(handle) = grpc_handle.write().await.take() {
                handle.abort();
            }
            if let Some(odgm) = odgm.write().await.take() {
                odgm.close().await;
            }
        });
    }
}

/// Helper functions for creating test data
pub mod test_data {
    use oprc_pb::{ObjData, ObjMeta, ValData, ValType};
    use std::collections::HashMap;

    #[allow(dead_code)] // Will be used for data operation tests
    pub fn create_test_object(object_id: u64, value: &str) -> ObjData {
        ObjData {
            metadata: Some(ObjMeta {
                cls_id: "test_cls".to_string(),
                partition_id: 0,
                object_id,
            }),
            entries: HashMap::from([(
                1,
                ValData {
                    data: value.as_bytes().to_vec(),
                    r#type: ValType::Byte as i32,
                },
            )]),
            event: None,
        }
    }

    #[allow(dead_code)] // Will be used for complex data operation tests
    pub fn create_complex_test_object(object_id: u64) -> ObjData {
        ObjData {
            metadata: Some(ObjMeta {
                cls_id: "test_cls".to_string(),
                partition_id: 0,
                object_id,
            }),
            entries: HashMap::from([
                (
                    1,
                    ValData {
                        data: "string_value".as_bytes().to_vec(),
                        r#type: ValType::Byte as i32,
                    },
                ),
                (
                    2,
                    ValData {
                        data: 42i64.to_le_bytes().to_vec(),
                        r#type: ValType::Byte as i32,
                    },
                ),
                (
                    3,
                    ValData {
                        data: 3.14f64.to_le_bytes().to_vec(),
                        r#type: ValType::Byte as i32,
                    },
                ),
                (
                    4,
                    ValData {
                        data: vec![1], // true as byte
                        r#type: ValType::Byte as i32,
                    },
                ),
            ]),
            event: None,
        }
    }
}

/// Assertion helpers for tests
pub mod assertions {
    use oprc_pb::ObjData;

    #[allow(dead_code)] // Will be used for data validation tests
    pub fn assert_object_equals(
        expected: &ObjData,
        actual: &ObjData,
        message: &str,
    ) {
        assert_eq!(
            expected.metadata.as_ref().unwrap().object_id,
            actual.metadata.as_ref().unwrap().object_id,
            "{}: Object IDs don't match",
            message
        );

        assert_eq!(
            expected.entries.len(),
            actual.entries.len(),
            "{}: Entry count doesn't match",
            message
        );

        for (key, expected_val) in &expected.entries {
            let actual_val = actual.entries.get(key).expect(&format!(
                "{}: Key {} not found in actual object",
                message, key
            ));
            assert_eq!(
                expected_val.data, actual_val.data,
                "{}: Value for key {} doesn't match",
                message, key
            );
        }
    }
}

/// Utilities for setting up test environments
pub mod setup {
    use super::TestConfig;

    #[allow(dead_code)] // Will be used for single node tests
    pub async fn create_single_node_config() -> TestConfig {
        TestConfig::new().await
    }

    #[allow(dead_code)] // Will be used when cluster tests are enabled
    pub async fn create_cluster_configs(node_count: u64) -> Vec<TestConfig> {
        let mut configs = Vec::new();
        let member_list: Vec<String> =
            (1..=node_count).map(|i| i.to_string()).collect();
        let members_str = member_list.join(",");

        for i in 1..=node_count {
            let mut config = TestConfig::with_node_id(i).await;
            config.odgm_config.members = Some(members_str.clone());
            configs.push(config);
        }

        configs
    }
}
