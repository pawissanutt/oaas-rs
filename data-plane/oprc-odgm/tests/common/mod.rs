use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use oprc_grpc::{
    CreateCollectionRequest, DataTrigger, FuncTrigger, ObjData, ObjMeta,
    ObjectEvent, SetObjectRequest, SingleObjectRequest, TriggerTarget, ValData,
    ValType, data_service_client::DataServiceClient,
};
use oprc_odgm::{ObjectDataGridManager, OdgmConfig};
use oprc_zenoh::pool::Pool;
use tracing::{debug, error, info};
use zenoh::Session;

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
                max_string_id_len: 160,
                enable_string_entry_keys: true,
                enable_granular_entry_storage: false,
                granular_prefetch_limit: 256,
                caps_queryable_enabled: true,
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

    /// Create a test collection with specified shard type
    #[allow(dead_code)] // Used by integration tests via env.create_test_collection
    pub async fn create_test_collection(
        &self,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.create_test_collection_with_shard_type(name, "basic")
            .await
    }

    /// Create a test collection with specified shard type
    #[allow(dead_code)] // Used by integration tests
    pub async fn create_test_collection_with_shard_type(
        &self,
        name: &str,
        shard_type: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let odgm_guard = self.odgm.read().await;
        let odgm = odgm_guard.as_ref().ok_or("ODGM not started")?;

        let collection_req = CreateCollectionRequest {
            name: name.to_string(),
            partition_count: 2,
            replica_count: 1,
            shard_type: shard_type.to_string(),
            shard_assignments: vec![],
            options: std::collections::HashMap::new(),
            invocations: None,
        };

        odgm.metadata_manager
            .create_collection(collection_req)
            .await?;

        // Wait for shards to be created (extra time for MST initialization)
        let wait_time = if shard_type == "mst" { 3000 } else { 1000 };
        tokio::time::sleep(Duration::from_millis(wait_time)).await;

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

        // Shutdown ODGM (this stops shard networks)
        if let Some(odgm) = self.odgm.write().await.take() {
            odgm.close().await;
        }

        // Close all Zenoh sessions
        if let Some(pool) = self.session_pool.write().await.take() {
            pool.close().await;
        }
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

/// Enhanced test context with event system integration and robust connection handling
/// Provides event testing capabilities with automatic port allocation, retry logic,
/// and Zenoh event subscriptions for integration tests.
#[allow(dead_code)]
pub struct EventTestContext {
    pub session: Session,
    pub client: DataServiceClient<tonic::transport::Channel>,
    pub env: TestEnvironment,
    pub config: TestConfig,
    pub test_id: String,
    pub collection_name: String,
}

#[allow(dead_code)]
impl EventTestContext {
    /// Create a new EventTestContext with automatic setup
    /// This includes ODGM startup, test collection creation, and gRPC client connection
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_with_shard_type("basic").await
    }

    /// Create a new EventTestContext with specified shard type
    /// Use "basic" for simple tests, "mst" for MST replication tests, "raft" for Raft tests
    pub async fn new_with_shard_type(
        shard_type: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique identifiers for this test instance
        let test_id = nanoid::nanoid!(8);
        let collection_name = format!("test_collection_{}", test_id);

        info!(
            "Starting EventTestContext initialization with test_id: {}, shard_type: {}",
            test_id, shard_type
        );

        // Setup ODGM config with events enabled and unique ports
        let http_port = Self::find_free_port().await;
        debug!("Setting up ODGM config with unique ports");
        let mut config = TestConfig::new().await;
        config.odgm_config.events_enabled = true;
        config.odgm_config.http_port = http_port;

        info!(
            "ODGM config created with events enabled: {}, HTTP port: {}",
            config.odgm_config.events_enabled, config.odgm_config.http_port
        );

        // Start ODGM server
        info!("Starting ODGM server on port {}", http_port);
        let env = TestEnvironment::new(config.clone()).await;
        env.start_odgm().await?;
        info!("ODGM server started on port {}", http_port);

        // Give the server a moment to fully start up
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Create test collection with specified shard type
        debug!(
            "Creating test collection: {} with shard type: {}",
            collection_name, shard_type
        );
        env.create_test_collection_with_shard_type(
            &collection_name,
            shard_type,
        )
        .await?;
        info!(
            "Test collection '{}' created with shard type: {}",
            collection_name, shard_type
        );

        // Create gRPC client with retry logic
        let client_addr =
            format!("http://127.0.0.1:{}", config.odgm_config.http_port);
        debug!("Connecting to gRPC client at: {}", client_addr);

        // Wait for server to be ready with retry logic
        let mut retries = 0;
        let max_retries = 30; // 30 retries * 100ms = 3 seconds max wait
        let client = loop {
            match DataServiceClient::connect(client_addr.clone()).await {
                Ok(client) => {
                    info!(
                        "gRPC client connected successfully after {} retries",
                        retries
                    );
                    break client;
                }
                Err(e) if retries < max_retries => {
                    debug!(
                        "Connection attempt {} failed: {}. Retrying in 100ms...",
                        retries + 1,
                        e
                    );
                    retries += 1;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    error!(
                        "Failed to connect after {} retries: {}",
                        max_retries, e
                    );
                    return Err(Box::new(e));
                }
            }
        };

        info!(
            "EventTestContext initialization completed for test_id: {}",
            test_id
        );
        let session = env.get_session().await;
        Ok(Self {
            session,
            client,
            env,
            config,
            test_id,
            collection_name,
        })
    }

    /// Find an available port for binding
    async fn find_free_port() -> u16 {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0")
            .expect("Failed to bind to random port");
        let addr = listener.local_addr().expect("Failed to get local addr");
        addr.port()
    }

    /// Create a Zenoh subscriber for event notifications
    /// This is used to listen for events triggered by data operations
    pub async fn create_subscriber(
        &self,
        event_type: &str,
    ) -> Result<
        zenoh::pubsub::Subscriber<
            zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>,
        >,
        Box<dyn std::error::Error>,
    > {
        let keyexpr = format!(
            "oprc/notification_service/1/async/{}_{}/*",
            event_type, self.test_id
        );
        info!("Creating subscriber for keyexpr: {}", keyexpr);
        debug!(
            "Subscribing to event type: {} for test_id: {}",
            event_type, self.test_id
        );

        let subscriber = self
            .session
            .declare_subscriber(keyexpr)
            .await
            .map_err(|e| format!("Failed to create subscriber: {}", e))?;
        info!(
            "Successfully created subscriber for event type: {} (test_id: {})",
            event_type, self.test_id
        );
        Ok(subscriber)
    }

    /// Create a data trigger for object lifecycle events (create, update, delete)
    pub fn create_data_trigger(
        &self,
        event_type: &str,
        fn_id: &str,
    ) -> DataTrigger {
        let unique_fn_id = format!("{}_{}", fn_id, self.test_id);
        debug!(
            "Creating data trigger for event_type: {}, fn_id: {}, test_id: {}",
            event_type, unique_fn_id, self.test_id
        );

        let mut data_trigger = DataTrigger::default();
        let target = TriggerTarget::stateless(
            "notification_service",
            1,
            unique_fn_id.clone(),
        );

        match event_type {
            "create" => {
                data_trigger.on_create.push(target);
                debug!("Added on_create trigger for fn_id: {}", unique_fn_id);
            }
            "update" => {
                data_trigger.on_update.push(target);
                debug!("Added on_update trigger for fn_id: {}", unique_fn_id);
            }
            "delete" => {
                data_trigger.on_delete.push(target);
                debug!("Added on_delete trigger for fn_id: {}", unique_fn_id);
            }
            _ => {
                error!("Unknown event type: {}", event_type);
                panic!("Unknown event type: {}", event_type);
            }
        }

        data_trigger
    }

    /// Create a data trigger used for string entry key events (same as numeric helper but kept for clarity)
    pub fn create_string_data_trigger(
        &self,
        event_type: &str,
        fn_id: &str,
    ) -> DataTrigger {
        self.create_data_trigger(event_type, fn_id)
    }

    /// Create a function trigger for function lifecycle events (complete, error)
    pub fn create_func_trigger(
        &self,
        event_type: &str,
        fn_id: &str,
    ) -> FuncTrigger {
        let unique_fn_id = format!("{}_{}", fn_id, self.test_id);
        debug!(
            "Creating function trigger for event_type: {}, fn_id: {}, test_id: {}",
            event_type, unique_fn_id, self.test_id
        );

        let mut func_trigger = FuncTrigger::default();
        let target = TriggerTarget::stateless(
            "notification_service",
            1,
            unique_fn_id.clone(),
        );

        match event_type {
            "complete" => {
                func_trigger.on_complete.push(target);
                debug!(
                    "Added on_complete function trigger for fn_id: {}",
                    unique_fn_id
                );
            }
            "error" => {
                func_trigger.on_error.push(target);
                debug!(
                    "Added on_error function trigger for fn_id: {}",
                    unique_fn_id
                );
            }
            _ => {
                error!("Unknown function event type: {}", event_type);
                panic!("Unknown function event type: {}", event_type);
            }
        }

        func_trigger
    }

    /// Create a test object with optional event triggers
    /// This is a helper for creating test data with consistent structure
    pub fn create_test_object(
        &self,
        object_id: u64,
        data: &[u8],
        object_event: Option<ObjectEvent>,
    ) -> ObjData {
        debug!(
            "Creating test object with id: {}, data_len: {} bytes, collection: {}",
            object_id,
            data.len(),
            self.collection_name
        );

        let mut entries = HashMap::new();
        entries.insert(
            1,
            ValData {
                data: data.to_vec(),
                r#type: ValType::Byte as i32,
            },
        );

        let obj_data = ObjData {
            metadata: Some(ObjMeta {
                cls_id: self.collection_name.clone(),
                partition_id: 1, // Use partition 1 for deterministic routing
                object_id,
                object_id_str: None,
            }),
            entries,
            event: object_event.clone(),
            entries_str: Default::default(),
        };

        if object_event.is_some() {
            debug!("Test object created with event triggers attached");
        } else {
            debug!("Test object created without event triggers");
        }

        obj_data
    }

    /// Set an object using the gRPC client
    /// This provides a convenient wrapper for object creation with proper error handling
    pub async fn set_object(
        &mut self,
        obj: ObjData,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata =
            obj.metadata.as_ref().ok_or("Object metadata is required")?;
        // For string ID usage we must set numeric id to 0 to satisfy service mutual exclusivity
        let object_id = if metadata.object_id_str.is_some() {
            0
        } else {
            metadata.object_id
        };
        let partition_id = metadata.partition_id as i32;

        info!(
            "Setting object: cls_id={}, partition_id={}, object_id={}",
            self.collection_name, partition_id, object_id
        );

        let result = self
            .client
            .set(SetObjectRequest {
                cls_id: self.collection_name.clone(),
                partition_id,
                object_id,
                object: Some(obj),
                object_id_str: None,
            })
            .await;

        match &result {
            Ok(_) => {
                info!("Successfully set object with id: {}", object_id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to set object {}: {}", object_id, e);
                Err(Box::new(e.clone()))
            }
        }
    }

    /// Set object supporting string ID (object_id_str) when provided in metadata.
    pub async fn set_object_flex(
        &mut self,
        obj: ObjData,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata =
            obj.metadata.as_ref().ok_or("Object metadata is required")?;
        let object_id = metadata.object_id;
        let partition_id = metadata.partition_id as i32;
        let object_id_str = metadata.object_id_str.clone();
        let result = self
            .client
            .set(SetObjectRequest {
                cls_id: self.collection_name.clone(),
                partition_id,
                object_id,
                object: Some(obj),
                object_id_str,
            })
            .await;
        match &result {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e.clone())),
        }
    }

    /// Delete an object using the gRPC client
    /// This provides a convenient wrapper for object deletion
    pub async fn delete_object(
        &mut self,
        obj: &ObjData,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata =
            obj.metadata.as_ref().ok_or("Object metadata is required")?;
        info!(
            "Deleting object: cls_id={}, partition_id={}, object_id={}",
            metadata.cls_id, metadata.partition_id, metadata.object_id
        );

        let result = self
            .client
            .delete(SingleObjectRequest {
                cls_id: self.collection_name.clone(),
                partition_id: metadata.partition_id,
                object_id: metadata.object_id,
                object_id_str: None,
            })
            .await;

        match &result {
            Ok(_) => {
                info!(
                    "Successfully deleted object with id: {}",
                    metadata.object_id
                );
                Ok(())
            }
            Err(e) => {
                error!("Failed to delete object {}: {}", metadata.object_id, e);
                Err(Box::new(e.clone()))
            }
        }
    }

    /// Wait for an event with timeout
    /// This is a helper for event-driven tests that need to wait for notifications
    pub async fn wait_for_event(
        &self,
        subscriber: &zenoh::pubsub::Subscriber<
            zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>,
        >,
        timeout: Duration,
    ) -> Result<zenoh::sample::Sample, Box<dyn std::error::Error>> {
        debug!("Waiting for event with timeout: {:?}", timeout);

        let result =
            tokio::time::timeout(timeout, subscriber.recv_async()).await;

        match result {
            Ok(Ok(sample)) => {
                info!(
                    "✅ Received event payload: {} bytes",
                    sample.payload().len()
                );
                debug!("Event sample key: {}", sample.key_expr());
                Ok(sample)
            }
            Ok(Err(e)) => {
                error!("Error receiving event: {}", e);
                Err(format!("Error receiving event: {}", e).into())
            }
            Err(_) => {
                error!("Timeout waiting for event after {:?}", timeout);
                Err("Timeout waiting for event".into())
            }
        }
    }

    /// Shutdown the test context and clean up resources
    pub async fn shutdown(&self) {
        info!(
            "Shutting down EventTestContext for test_id: {}",
            self.test_id
        );
        self.env.shutdown().await;
        info!(
            "EventTestContext shutdown completed for test_id: {}",
            self.test_id
        );
    }
}

impl Drop for EventTestContext {
    fn drop(&mut self) {
        let env = self.env.clone();
        let test_id = self.test_id.clone();

        tokio::spawn(async move {
            info!(
                "Cleaning up EventTestContext resources for test_id: {}",
                test_id
            );
            env.shutdown().await;
        });
    }
}

/// Helper functions for creating test data
pub mod test_data {
    use oprc_grpc::{ObjData, ObjMeta, ValData, ValType};
    use std::collections::HashMap;

    #[allow(dead_code)] // Will be used for data operation tests
    pub fn create_test_object(object_id: u64, value: &str) -> ObjData {
        ObjData {
            metadata: Some(ObjMeta {
                cls_id: "test_cls".to_string(),
                partition_id: 0,
                object_id,
                object_id_str: None,
            }),
            entries: HashMap::from([(
                1,
                ValData {
                    data: value.as_bytes().to_vec(),
                    r#type: ValType::Byte as i32,
                },
            )]),
            event: None,
            entries_str: Default::default(),
        }
    }

    #[allow(dead_code)] // Will be used for complex data operation tests
    pub fn create_complex_test_object(object_id: u64) -> ObjData {
        ObjData {
            metadata: Some(ObjMeta {
                cls_id: "test_cls".to_string(),
                partition_id: 0,
                object_id,
                object_id_str: None,
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
                        data: std::f64::consts::PI.to_le_bytes().to_vec(),
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
            entries_str: Default::default(),
        }
    }
}

/// Assertion helpers for tests
pub mod assertions {
    use oprc_grpc::ObjData;

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
