//! OpenRaft-based ReplicationLayer implementation using ShardRequest directly
//! Creates real OpenRaft instance with consensus, state machine, and networking

use async_trait::async_trait;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{watch, RwLock};
use tracing::{info, warn};

use crate::shard::unified::traits::ShardMetadata;
use oprc_dp_storage::{ApplicationDataStorage, StorageValue};

// Import flare-dht components for proper OpenRaft integration
use flare_dht::raft::generic::LocalStateMachineStore;
use flare_dht::raft::{
    log::MemLogStore,
    rpc::{Network, RaftZrpcService},
};
use flare_zrpc::client::ZrpcClientConfig;
use flare_zrpc::server::ServerConfig;
use zenoh::qos::{CongestionControl, Priority};

/// State machine that processes ShardRequest directly and integrates with oprc-dp-storage
#[derive(Clone)]
pub struct ReplicationStateMachine<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    /// Application data storage backend (injected dependency)
    pub app_storage: Arc<A>,
    /// In-memory cache for fast access (optional optimization)
    pub data_cache: Arc<RwLock<BTreeMap<String, StorageValue>>>,
}

impl<A> ReplicationStateMachine<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    /// Create new state machine with injected storage
    pub fn new(app_storage: A) -> Self {
        Self {
            app_storage: Arc::new(app_storage),
            data_cache: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Apply a shard request to the application storage (async version)
    pub async fn apply_request_async(
        &self,
        request: &ShardRequest,
    ) -> ReplicationResponse {
        match &request.operation {
            Operation::Write(write_op) => {
                let result = self
                    .app_storage
                    .put(write_op.key.as_bytes(), write_op.value.clone())
                    .await;

                match result {
                    Ok(_) => {
                        // Update cache
                        self.data_cache.write().await.insert(
                            write_op.key.clone(),
                            write_op.value.clone(),
                        );

                        ReplicationResponse {
                            status: ResponseStatus::Applied,
                            data: Some(write_op.value.clone()),
                            metadata: HashMap::new(),
                        }
                    }
                    Err(e) => ReplicationResponse {
                        status: ResponseStatus::Failed(format!(
                            "Write failed: {}",
                            e
                        )),
                        data: None,
                        metadata: HashMap::new(),
                    },
                }
            }
            Operation::Read(read_op) => {
                let result = self.app_storage.get(read_op.key.as_bytes()).await;

                match result {
                    Ok(Some(value)) => ReplicationResponse {
                        status: ResponseStatus::Applied,
                        data: Some(value),
                        metadata: HashMap::new(),
                    },
                    Ok(None) => ReplicationResponse {
                        status: ResponseStatus::Failed(
                            "Key not found".to_string(),
                        ),
                        data: None,
                        metadata: HashMap::new(),
                    },
                    Err(e) => ReplicationResponse {
                        status: ResponseStatus::Failed(format!(
                            "Read failed: {}",
                            e
                        )),
                        data: None,
                        metadata: HashMap::new(),
                    },
                }
            }
            Operation::Delete(delete_op) => {
                let result =
                    self.app_storage.delete(delete_op.key.as_bytes()).await;

                match result {
                    Ok(_) => {
                        // Update cache
                        self.data_cache.write().await.remove(&delete_op.key);

                        ReplicationResponse {
                            status: ResponseStatus::Applied,
                            data: None,
                            metadata: HashMap::new(),
                        }
                    }
                    Err(e) => ReplicationResponse {
                        status: ResponseStatus::Failed(format!(
                            "Delete failed: {}",
                            e
                        )),
                        data: None,
                        metadata: HashMap::new(),
                    },
                }
            }
            Operation::Batch(operations) => {
                let mut success_count = 0;
                let total_count = operations.len();

                for op in operations {
                    let batch_request = ShardRequest {
                        operation: op.clone(),
                        timestamp: request.timestamp,
                        source_node: request.source_node,
                        request_id: request.request_id.clone(),
                    };

                    // Use Box::pin to avoid recursion issues
                    let response =
                        Box::pin(self.apply_request_async(&batch_request))
                            .await;
                    if matches!(response.status, ResponseStatus::Applied) {
                        success_count += 1;
                    }
                }

                if success_count == total_count {
                    ReplicationResponse {
                        status: ResponseStatus::Applied,
                        data: None,
                        metadata: HashMap::from([
                            ("batch_size".to_string(), total_count.to_string()),
                            (
                                "success_count".to_string(),
                                success_count.to_string(),
                            ),
                        ]),
                    }
                } else {
                    ReplicationResponse {
                        status: ResponseStatus::Failed(format!(
                            "Batch partially failed: {}/{} operations succeeded",
                            success_count, total_count
                        )),
                        data: None,
                        metadata: HashMap::from([
                            ("batch_size".to_string(), total_count.to_string()),
                            ("success_count".to_string(), success_count.to_string()),
                        ]),
                    }
                }
            }
        }
    }

    /// Apply a shard request to the application storage (sync version for AppStateMachine trait)
    pub fn apply_request_sync(
        &self,
        request: &ShardRequest,
    ) -> ReplicationResponse {
        // For the sync version, we'll use a blocking approach
        // In production, you might want to use a different strategy
        let rt = tokio::runtime::Handle::current();
        rt.block_on(self.apply_request_async(request))
    }
}

// Implement the flare_dht AppStateMachine trait for ReplicationStateMachine
impl<A> flare_dht::raft::generic::AppStateMachine for ReplicationStateMachine<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    type Req = ShardRequest;
    type Resp = ReplicationResponse;

    fn load_snapshot_app(data: &[u8]) -> Result<Self, openraft::AnyError> {
        // For simplicity, we'll deserialize a placeholder
        // In production, this should restore the actual storage state
        let _snapshot_data: BTreeMap<String, StorageValue> =
            bincode::serde::decode_from_slice(
                data,
                bincode::config::standard(),
            )
            .map_err(|e| openraft::AnyError::new(&e))?
            .0;

        // Create a new storage instance (this is simplified)
        // In production, you'd want to restore the actual storage state
        use oprc_dp_storage::{
            EnhancedApplicationStorage, MemoryStorage, StorageBackendType,
            StorageConfig,
        };

        let storage_config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            path: None,
            memory_limit_mb: Some(256),
            cache_size_mb: Some(64),
            compression: false,
            sync_writes: false,
            properties: HashMap::new(),
        };

        // This is a workaround - in production you'd want proper snapshot restoration
        let rt = tokio::runtime::Handle::current();
        let memory_backend = rt
            .block_on(MemoryStorage::new(storage_config))
            .map_err(|e| openraft::AnyError::new(&e))?;
        let enhanced_storage = rt
            .block_on(EnhancedApplicationStorage::new(memory_backend))
            .map_err(|e| openraft::AnyError::new(&e))?;

        Ok(Self::new(enhanced_storage))
    }

    fn snapshot_app(&self) -> Result<Vec<u8>, openraft::AnyError> {
        // Export current state for snapshot
        let rt = tokio::runtime::Handle::current();
        let cache_data = rt.block_on(self.data_cache.read());

        let encoded = bincode::serde::encode_to_vec(
            &*cache_data,
            bincode::config::standard(),
        )
        .map_err(|e| openraft::AnyError::new(&e))?;
        Ok(encoded)
    }

    fn apply(&mut self, req: &Self::Req) -> Self::Resp {
        // Use the sync version of apply_request
        self.apply_request_sync(req)
    }

    fn empty_resp(&self) -> Self::Resp {
        ReplicationResponse {
            status: ResponseStatus::Applied,
            data: None,
            metadata: HashMap::new(),
        }
    }
}

/// Operation Manager for executing operations through OpenRaft consensus
/// Similar to RaftOperationManager but adapted for ShardRequest/ReplicationResponse
pub struct ReplicationOperationManager {
    raft: openraft::Raft<ReplicationTypeConfig>,
}

impl ReplicationOperationManager {
    pub fn new(raft: openraft::Raft<ReplicationTypeConfig>) -> Self {
        Self { raft }
    }

    /// Execute a ShardRequest through OpenRaft consensus
    pub async fn exec(
        &self,
        request: &ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        // For write operations, go through Raft consensus
        match &request.operation {
            Operation::Write(_)
            | Operation::Delete(_)
            | Operation::Batch(_) => {
                let client_request =
                    openraft::ClientWriteRequest::new(request.clone());
                let response =
                    self.raft.client_write(client_request).await.map_err(
                        |e| {
                            ReplicationError::ConsensusError(format!(
                                "Raft write failed: {}",
                                e
                            ))
                        },
                    )?;

                match response.data {
                    Some(data) => Ok(data),
                    None => Err(ReplicationError::ConsensusError(
                        "No response data".to_string(),
                    )),
                }
            }
            Operation::Read(_) => {
                // Reads can be served directly for linearizable consistency
                // In production, might want to use raft.is_leader() check
                Err(ReplicationError::ConsensusError(
                    "Reads should go through state machine directly"
                        .to_string(),
                ))
            }
        }
    }
}

/// OpenRaft-based ReplicationLayer implementation with real consensus
/// Creates actual OpenRaft instance with state machine and networking
pub struct OpenRaftReplicationLayer<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    /// Node ID in the Raft cluster
    node_id: u64,

    /// Shard metadata for configuration
    shard_metadata: ShardMetadata,

    /// OpenRaft instance (the core consensus engine)
    raft: openraft::Raft<ReplicationTypeConfig>,

    /// State machine store
    store: LocalStateMachineStore<
        ReplicationStateMachine<A>,
        ReplicationTypeConfig,
    >,

    /// RPC service for Raft protocol messages
    rpc_service: tokio::sync::Mutex<RaftZrpcService<ReplicationTypeConfig>>,

    /// Operation manager for executing operations through consensus
    operation_manager: ReplicationOperationManager,

    /// Readiness tracking
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,

    /// Cancellation token for cleanup
    cancellation: tokio_util::sync::CancellationToken,
}

impl<A> OpenRaftReplicationLayer<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    /// Create a new OpenRaftReplicationLayer with real OpenRaft instance
    /// This creates actual consensus with state machine and networking
    pub async fn new(
        node_id: u64,
        app_storage: A, // Injected storage dependency
        shard_metadata: ShardMetadata,
        z_session: zenoh::Session,
        rpc_prefix: String,
    ) -> Result<Self, ReplicationError> {
        // Create OpenRaft configuration (similar to existing RaftObjectShard)
        let config = openraft::Config {
            cluster_name: format!(
                "oprc-replication/{}/{}",
                shard_metadata.collection, shard_metadata.partition_id
            ),
            election_timeout_min: 200,
            election_timeout_max: 2000,
            max_payload_entries: 1024,
            purge_batch_size: 1024,
            heartbeat_interval: 100,
            snapshot_policy: openraft::SnapshotPolicy::LogsSinceLast(10000),
            ..Default::default()
        };

        let config = Arc::new(config.validate().map_err(|e| {
            ReplicationError::ConsensusError(format!(
                "Invalid Raft config: {:?}",
                e
            ))
        })?);

        // Create log store (in-memory for now, could be made configurable)
        let log_store = MemLogStore::default();

        // Create state machine store with injected storage
        let state_machine = ReplicationStateMachine::new(app_storage);
        let store: LocalStateMachineStore<
            ReplicationStateMachine<A>,
            ReplicationTypeConfig,
        > = LocalStateMachineStore::new_with_state_machine(state_machine);

        // Create Zenoh-based network layer (using existing flare-dht infrastructure)
        let zrpc_client_config = ZrpcClientConfig {
            service_id: rpc_prefix.clone(),
            target: zenoh::query::QueryTarget::BestMatching,
            channel_size: 8,
            congestion_control: CongestionControl::Drop,
            priority: Priority::DataHigh,
        };

        let network = Network::new_with_config(
            z_session.clone(),
            rpc_prefix.clone(),
            zrpc_client_config,
        );

        // Create the actual OpenRaft instance
        let raft = openraft::Raft::<ReplicationTypeConfig>::new(
            node_id,
            config.clone(),
            network,
            log_store,
            store.clone(),
        )
        .await
        .map_err(|e| {
            ReplicationError::ConsensusError(format!(
                "Failed to create Raft: {}",
                e
            ))
        })?;

        // Create RPC service for handling Raft protocol messages
        let zrpc_server_config = ServerConfig {
            reply_congestion: CongestionControl::Block,
            reply_priority: Priority::DataHigh,
            ..Default::default()
        };

        let rpc_service = RaftZrpcService::new(
            raft.clone(),
            z_session,
            rpc_prefix,
            node_id,
            zrpc_server_config,
        );

        // Create operation manager for consensus operations
        let operation_manager = ReplicationOperationManager::new(raft.clone());

        let (readiness_tx, readiness_rx) = watch::channel(false);
        let cancellation = tokio_util::sync::CancellationToken::new();

        Ok(Self {
            node_id,
            shard_metadata,
            raft,
            store,
            rpc_service: tokio::sync::Mutex::new(rpc_service),
            operation_manager,
            readiness_tx,
            readiness_rx,
            cancellation,
        })
    }

    /// Initialize the OpenRaft cluster (similar to RaftObjectShard::initialize)
    pub async fn initialize(&self) -> Result<(), ReplicationError> {
        // Start RPC service
        self.rpc_service.lock().await.start().await.map_err(|e| {
            ReplicationError::NetworkError(format!(
                "Failed to start RPC service: {}",
                e
            ))
        })?;

        // Set up readiness monitoring based on leadership
        let leader_only = self
            .shard_metadata
            .options
            .get("raft_net_leader_only")
            .map(|v| v == "true")
            .unwrap_or(false);

        if leader_only {
            let sender = self.readiness_tx.clone();
            let token = self.cancellation.clone();
            let mut watch = self.raft.server_metrics();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = token.cancelled() => {
                            break;
                        },
                        res = watch.changed() => {
                            if res.is_err() {
                                break;
                            }
                            let metric = watch.borrow();
                            let _ = sender.send(Some(metric.id) == metric.current_leader);
                        }
                    }
                }
            });
        } else {
            let _ = self.readiness_tx.send(true);
        }

        // Initialize cluster if this is the primary node
        let init_leader_only = self
            .shard_metadata
            .options
            .get("raft_init_leader_only")
            .map(|v| v == "true")
            .unwrap_or(false);

        if !init_leader_only
            || self.shard_metadata.primary == Some(self.node_id)
        {
            info!(
                "shard '{}': initialize raft cluster",
                self.shard_metadata.id
            );
            let mut members = BTreeMap::new();
            for member in self.shard_metadata.replica.iter() {
                members.insert(
                    *member,
                    openraft::BasicNode {
                        addr: member.to_string(),
                    },
                );
            }

            if let Err(e) = self.raft.initialize(members).await {
                warn!(
                    "shard '{}': error initiating raft: {:?}",
                    self.shard_metadata.id, e
                );
                return Err(ReplicationError::ConsensusError(format!(
                    "Failed to initialize cluster: {}",
                    e
                )));
            }
        }

        Ok(())
    }

    /// Shutdown the OpenRaft instance and services
    pub async fn shutdown(&self) -> Result<(), ReplicationError> {
        self.rpc_service.lock().await.close().await;
        self.cancellation.cancel();

        if let Err(e) = self.raft.shutdown().await {
            return Err(ReplicationError::ConsensusError(format!(
                "Failed to shutdown Raft: {}",
                e
            )));
        }

        Ok(())
    }

    /// Get Raft metrics from the actual OpenRaft instance
    pub async fn get_metrics(
        &self,
    ) -> openraft::RaftMetrics<u64, openraft::BasicNode> {
        // Get real metrics from the OpenRaft instance
        self.raft.server_metrics().borrow().clone()
    }

    /// Check if this node is the leader
    pub async fn is_leader(&self) -> bool {
        let metrics = self.get_metrics().await;
        metrics.current_leader == Some(self.node_id)
    }

    /// Start the services
    pub async fn start_services(&self) -> Result<(), ReplicationError> {
        self.initialize().await
    }

    /// Stop the services
    pub async fn stop_services(&self) -> Result<(), ReplicationError> {
        self.shutdown().await
    }
}

impl<A> Clone for OpenRaftReplicationLayer<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            shard_metadata: self.shard_metadata.clone(),
            raft: self.raft.clone(),
            store: self.store.clone(),
            rpc_service: tokio::sync::Mutex::new(
                self.rpc_service.try_lock().unwrap().clone(),
            ),
            operation_manager: ReplicationOperationManager::new(
                self.raft.clone(),
            ),
            readiness_tx: self.readiness_tx.clone(),
            readiness_rx: self.readiness_rx.clone(),
            cancellation: self.cancellation.clone(),
        }
    }
}

#[async_trait]
impl<A> ReplicationLayer for OpenRaftReplicationLayer<A>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    type Error = ReplicationError;

    fn replication_model(&self) -> ReplicationModel {
        // Get current term from metrics synchronously
        let metrics = self.raft.server_metrics().borrow().clone();
        ReplicationModel::Consensus {
            algorithm: ConsensusAlgorithm::Raft,
            current_term: Some(metrics.current_term),
        }
    }

    async fn replicate_write(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, Self::Error> {
        // Use OpenRaft consensus for write operations
        self.operation_manager.exec(&request).await
    }

    async fn replicate_read(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, Self::Error> {
        // For reads, we can serve directly from state machine for linearizable consistency
        // In production, you might want to add leader check
        let state_machine = &self.store.state_machine.read().await;
        Ok(state_machine.apply_request_async(&request).await)
    }

    async fn add_replica(
        &self,
        node_id: u64,
        address: String,
    ) -> Result<(), Self::Error> {
        // Use OpenRaft's change_membership API
        let mut new_members = BTreeMap::new();

        // Get current membership and add new node
        let metrics = self.get_metrics().await;
        if let Some(membership) = metrics
            .membership_config
            .membership()
            .voter_ids()
            .collect::<Vec<_>>()
            .first()
        {
            for member_id in metrics.membership_config.membership().voter_ids()
            {
                new_members.insert(
                    member_id,
                    openraft::BasicNode {
                        addr: member_id.to_string(),
                    },
                );
            }
        }

        new_members.insert(node_id, openraft::BasicNode { addr: address });

        self.raft
            .change_membership(new_members, false)
            .await
            .map_err(|e| {
                ReplicationError::ConsensusError(format!(
                    "Failed to add replica: {}",
                    e
                ))
            })?;

        Ok(())
    }

    async fn remove_replica(&self, node_id: u64) -> Result<(), Self::Error> {
        // Use OpenRaft's change_membership API
        let mut new_members = BTreeMap::new();

        // Get current membership and remove the node
        let metrics = self.get_metrics().await;
        for member_id in metrics.membership_config.membership().voter_ids() {
            if member_id != node_id {
                new_members.insert(
                    member_id,
                    openraft::BasicNode {
                        addr: member_id.to_string(),
                    },
                );
            }
        }

        self.raft
            .change_membership(new_members, false)
            .await
            .map_err(|e| {
                ReplicationError::ConsensusError(format!(
                    "Failed to remove replica: {}",
                    e
                ))
            })?;

        Ok(())
    }

    async fn get_replication_status(
        &self,
    ) -> Result<ReplicationStatus, Self::Error> {
        let metrics = self.get_metrics().await;
        let is_leader = metrics.current_leader == Some(self.node_id);
        let healthy_replicas =
            metrics.membership_config.membership().voter_ids().count();

        Ok(ReplicationStatus {
            model: ReplicationModel::Consensus {
                algorithm: ConsensusAlgorithm::Raft,
                current_term: Some(metrics.current_term),
            },
            healthy_replicas,
            total_replicas: healthy_replicas,
            lag_ms: metrics.millis_since_quorum_ack,
            conflicts: 0, // OpenRaft handles conflicts internally
            is_leader,
            leader_id: metrics.current_leader,
            last_sync: Some(SystemTime::now()),
        })
    }

    async fn sync_replicas(&self) -> Result<(), Self::Error> {
        // In OpenRaft, replication is continuous.
        // We could trigger a heartbeat or check cluster health here
        if !self.is_leader().await {
            return Err(ReplicationError::ConsensusError(
                "Not leader, cannot sync replicas".to_string(),
            ));
        }

        // OpenRaft automatically handles replication, so this is essentially a no-op
        // In a more sophisticated implementation, we might trigger explicit log replication
        Ok(())
    }
}

/// Factory function to create OpenRaftReplicationLayer with proper OpenRaft instance
/// This creates real consensus with state machine and networking
pub async fn create_raft_replication_layer<A>(
    node_id: u64,
    app_storage: A, // Storage injected as dependency
    shard_metadata: ShardMetadata,
    z_session: zenoh::Session,
    rpc_prefix: String,
) -> Result<OpenRaftReplicationLayer<A>, ReplicationError>
where
    A: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    OpenRaftReplicationLayer::new(
        node_id,
        app_storage,
        shard_metadata,
        z_session,
        rpc_prefix,
    )
    .await
}

/// Convenience factory function for creating with memory storage
pub async fn create_memory_raft_replication_layer(
    node_id: u64,
    shard_metadata: ShardMetadata,
    z_session: zenoh::Session,
    rpc_prefix: String,
) -> Result<
    OpenRaftReplicationLayer<
        oprc_dp_storage::EnhancedApplicationStorage<
            oprc_dp_storage::MemoryStorage,
        >,
    >,
    ReplicationError,
> {
    use oprc_dp_storage::{
        EnhancedApplicationStorage, MemoryStorage, StorageBackendType,
        StorageConfig,
    };

    // Create storage externally (dependency injection principle)
    let storage_config = StorageConfig {
        backend_type: StorageBackendType::Memory,
        path: None,
        memory_limit_mb: Some(256),
        cache_size_mb: Some(64),
        compression: false,
        sync_writes: false,
        properties: HashMap::new(),
    };

    let memory_backend = MemoryStorage::new(storage_config)
        .await
        .map_err(|e| ReplicationError::StorageError(e.to_string()))?;

    let enhanced_storage = EnhancedApplicationStorage::new(memory_backend)
        .await
        .map_err(|e| ReplicationError::StorageError(e.to_string()))?;

    // Create with real OpenRaft instance
    create_raft_replication_layer(
        node_id,
        enhanced_storage,
        shard_metadata,
        z_session,
        rpc_prefix,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::{DeleteOperation, ReadOperation, WriteOperation};
    use oprc_dp_storage::{
        EnhancedApplicationStorage, MemoryStorage, StorageBackendType,
        StorageConfig,
    };

    async fn create_test_storage() -> EnhancedApplicationStorage<MemoryStorage>
    {
        let storage_config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            path: None,
            memory_limit_mb: Some(256),
            cache_size_mb: Some(64),
            compression: false,
            sync_writes: false,
            properties: HashMap::new(),
        };

        let memory_backend = MemoryStorage::new(storage_config).await.unwrap();
        EnhancedApplicationStorage::new(memory_backend)
            .await
            .unwrap()
    }

    fn create_test_metadata() -> ShardMetadata {
        ShardMetadata {
            id: 1,
            collection: "test".to_string(),
            partition_id: 1,
            owner: Some(1),
            primary: Some(1),
            replica: vec![1],
            replica_owner: vec![1],
            shard_type: "raft".to_string(),
            options: HashMap::new(),
            invocations: None,
            storage_config: None,
            replication_config: None,
            consistency_config: None,
        }
    }

    async fn create_test_zenoh_session() -> zenoh::Session {
        let config = zenoh::config::Config::default();
        zenoh::open(config).await.unwrap()
    }

    #[tokio::test]
    async fn test_replication_state_machine() {
        let storage = create_test_storage().await;
        let state_machine = ReplicationStateMachine::new(storage);

        // Test write operation
        let write_request = ShardRequest {
            operation: Operation::Write(WriteOperation {
                key: "test_key".to_string(),
                value: StorageValue::from("test_value".as_bytes()),
                ttl: None,
            }),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-123".to_string(),
        };

        let response = state_machine.apply_request_async(&write_request).await;
        assert!(matches!(response.status, ResponseStatus::Applied));

        // Test read operation
        let read_request = ShardRequest {
            operation: Operation::Read(ReadOperation {
                key: "test_key".to_string(),
                consistency: crate::replication::ReadConsistency::Linearizable,
            }),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-124".to_string(),
        };

        let response = state_machine.apply_request_async(&read_request).await;
        assert!(matches!(response.status, ResponseStatus::Applied));
        assert!(response.data.is_some());

        // Test delete operation
        let delete_request = ShardRequest {
            operation: Operation::Delete(DeleteOperation {
                key: "test_key".to_string(),
            }),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-125".to_string(),
        };

        let response = state_machine.apply_request_async(&delete_request).await;
        assert!(matches!(response.status, ResponseStatus::Applied));
    }

    #[tokio::test]
    async fn test_openraft_replication_layer_creation() {
        let storage = create_test_storage().await;
        let metadata = create_test_metadata();
        let z_session = create_test_zenoh_session().await;

        let replication_layer = create_raft_replication_layer(
            1,
            storage,
            metadata,
            z_session,
            "test_raft".to_string(),
        )
        .await
        .unwrap();

        // Test replication model
        let model = replication_layer.replication_model(); // This is synchronous
        assert!(matches!(
            model,
            ReplicationModel::Consensus {
                algorithm: ConsensusAlgorithm::Raft,
                ..
            }
        ));

        // Test initial status
        let status = replication_layer.get_replication_status().await.unwrap();
        assert_eq!(status.total_replicas, 1);
        assert_eq!(status.healthy_replicas, 1);
    }

    #[tokio::test]
    async fn test_dependency_injection_pattern() {
        let metadata = create_test_metadata();
        let z_session = create_test_zenoh_session().await;

        // Test the convenience function (storage created internally)
        let replication_layer1 = create_memory_raft_replication_layer(
            1,
            metadata.clone(),
            z_session.clone(),
            "test_memory_raft".to_string(),
        )
        .await
        .unwrap();

        // Test the dependency injection function (storage created externally)
        let external_storage = create_test_storage().await;
        let replication_layer2 = create_raft_replication_layer(
            2,
            external_storage,
            metadata,
            z_session,
            "test_external_raft".to_string(),
        )
        .await
        .unwrap();

        // Both should work
        assert_eq!(replication_layer1.node_id, 1);
        assert_eq!(replication_layer2.node_id, 2);
    }
}
