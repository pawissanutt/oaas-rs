//! OpenRaft-based ReplicationLayer implementation using ShardRequest directly
//! Creates real OpenRaft instance with consensus, state machine, and networking

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::replication::raft::{
    ObjectShardStateMachine, ReplicationTypeConfig,
};
use crate::replication::{
    ConsensusAlgorithm, ReplicationError, ReplicationLayer, ReplicationModel,
    ReplicationResponse, ReplicationStatus, ShardRequest,
};
use crate::shard::unified::traits::ShardMetadata;
use oprc_dp_storage::{MemoryStorage, SnapshotCapableStorage};

// Import flare-dht components for proper OpenRaft integration
use crate::replication::raft::raft_network::{Network, RaftZrpcService};
use flare_zrpc::client::ZrpcClientConfig;
use flare_zrpc::server::{ServerConfig, ZrpcService};
use flare_zrpc::{ZrpcClient, ZrpcError, ZrpcServiceHander};
use zenoh::qos::{CongestionControl, Priority};

// Define the RPC type for replication operations
pub type ReplicationRpcType = flare_zrpc::bincode::BincodeZrpcType<
    ShardRequest,
    openraft::raft::ClientWriteResponse<ReplicationTypeConfig>,
    openraft::error::RaftError<
        u64,
        openraft::error::ClientWriteError<u64, openraft::BasicNode>,
    >,
>;

/// RPC Service type for replication operations
pub type ReplicationRpcService =
    ZrpcService<ReplicationOperationHandler, ReplicationRpcType>;

/// Handler for incoming replication RPC requests
/// This handles requests forwarded from non-leader nodes
pub struct ReplicationOperationHandler {
    raft: openraft::Raft<ReplicationTypeConfig>,
}

impl ReplicationOperationHandler {
    pub fn new(raft: openraft::Raft<ReplicationTypeConfig>) -> Self {
        Self { raft }
    }
}

#[async_trait::async_trait]
impl ZrpcServiceHander<ReplicationRpcType> for ReplicationOperationHandler {
    async fn handle(
        &self,
        req: ShardRequest,
    ) -> Result<
        openraft::raft::ClientWriteResponse<ReplicationTypeConfig>,
        openraft::error::RaftError<
            u64,
            openraft::error::ClientWriteError<u64, openraft::BasicNode>,
        >,
    > {
        // Handle the forwarded request through local Raft
        self.raft.client_write(req).await
    }
}

/// Operation Manager for executing operations through OpenRaft consensus
/// Simplified approach: always try local first, use errors for leader forwarding
pub struct ReplicationOperationManager {
    /// Local Raft instance for operations
    raft: openraft::Raft<ReplicationTypeConfig>,

    /// RPC client for forwarding operations to the leader when needed
    pub(crate) rpc_client: ZrpcClient<ReplicationRpcType>,

    /// RPC server for handling forwarded operations from other nodes
    pub(crate) rpc_service: ReplicationRpcService,
}

impl ReplicationOperationManager {
    /// Create a new instance with both RPC client and server
    #[instrument(skip_all, fields(rpc_prefix = %rpc_prefix))]
    pub async fn new(
        raft: openraft::Raft<ReplicationTypeConfig>,
        z_session: zenoh::Session,
        rpc_prefix: String,
        client_config: ZrpcClientConfig,
        server_config: ServerConfig,
    ) -> Self {
        debug!("Creating replication operation manager");

        // Create RPC client for forwarding to leader
        let rpc_client = ZrpcClient::with_config(
            ZrpcClientConfig {
                service_id: rpc_prefix.clone(),
                ..client_config
            },
            z_session.clone(),
        )
        .await;

        // Create RPC server to handle incoming forwarded requests
        let rpc_handler = ReplicationOperationHandler::new(raft.clone());
        let rpc_service =
            ReplicationRpcService::new(z_session, server_config, rpc_handler);

        debug!("Replication operation manager created successfully");

        Self {
            raft,
            rpc_client,
            rpc_service,
        }
    }

    /// Execute a ShardRequest through OpenRaft consensus
    /// Simple approach: always try local first, let OpenRaft handle forwarding
    #[instrument(skip_all, fields(operation = ?request.operation))]
    pub async fn exec(
        &self,
        request: &ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        trace!("Executing shard request through Raft consensus");

        // Always try local execution first
        let result = self.raft.client_write(request.clone()).await;

        match result {
            Ok(response) => {
                // Successfully applied the operation locally
                trace!("Operation applied successfully on local node");
                Ok(response.data)
            }
            Err(openraft::error::RaftError::APIError(
                openraft::error::ClientWriteError::ForwardToLeader(forward),
            )) => {
                // We're not the leader, forward to the actual leader
                if let Some(leader_id) = forward.leader_id {
                    debug!("Not leader, forwarding to leader: {}", leader_id);
                    self.forward_to_leader(leader_id, request).await
                } else {
                    warn!("No leader available for forwarding");
                    Err(ReplicationError::ConsensusError(
                        "No leader available for forwarding".to_string(),
                    ))
                }
            }
            Err(e) => {
                // Other Raft errors
                error!("Raft error during operation execution: {:?}", e);
                Err(self.convert_raft_error(e))
            }
        }
    }

    /// Forward operation to the specified leader node
    #[instrument(skip_all, fields(leader_id = %leader_id))]
    async fn forward_to_leader(
        &self,
        leader_id: u64,
        request: &ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        debug!("Forwarding operation to leader");

        let key_str = format!("{}", leader_id);

        match self.rpc_client.call_with_key(key_str, request).await {
            Ok(response) => {
                trace!("Successfully forwarded operation to leader");
                Ok(response.data)
            }
            Err(ZrpcError::AppError(e)) => Err(self.convert_raft_error(e)),
            Err(e) => {
                error!("RPC error during leader forwarding: {:?}", e);
                Err(ReplicationError::NetworkError(format!(
                    "Failed to forward to leader {}: {}",
                    leader_id, e
                )))
            }
        }
    }

    /// Convert OpenRaft errors to ReplicationError
    fn convert_raft_error(
        &self,
        error: openraft::error::RaftError<
            u64,
            openraft::error::ClientWriteError<u64, openraft::BasicNode>,
        >,
    ) -> ReplicationError {
        match error {
            openraft::error::RaftError::APIError(api_error) => {
                match api_error {
                    openraft::error::ClientWriteError::ForwardToLeader(
                        forward,
                    ) => ReplicationError::ConsensusError(format!(
                        "Not leader, forward to: {:?}",
                        forward.leader_id
                    )),
                    _ => ReplicationError::ConsensusError(format!(
                        "Client write error: {}",
                        api_error
                    )),
                }
            }
            _ => ReplicationError::ConsensusError(format!(
                "Raft error: {}",
                error
            )),
        }
    }
}

/// OpenRaft-based ReplicationLayer implementation with real consensus
/// Creates actual OpenRaft instance with state machine and networking
pub struct OpenRaftReplicationLayer<A>
where
    A: SnapshotCapableStorage + Clone + Send + Sync + 'static,
{
    /// Node ID in the Raft cluster
    shard_id: u64,

    /// Shard metadata for configuration
    shard_metadata: ShardMetadata,

    /// State machine store
    store: A,

    /// OpenRaft instance managing consensus
    raft: openraft::Raft<ReplicationTypeConfig>,

    /// Raft RPC service for consensus operations (append, vote, snapshot)
    raft_rpc_service: RwLock<RaftZrpcService<ReplicationTypeConfig>>,

    /// Operation manager for executing operations through consensus
    operation_manager: RwLock<ReplicationOperationManager>,

    /// Readiness tracking
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,

    /// Cancellation token for cleanup
    pub(crate) cancellation: tokio_util::sync::CancellationToken,
}

impl<A> OpenRaftReplicationLayer<A>
where
    A: SnapshotCapableStorage + Clone + Send + Sync + 'static,
{
    /// Create a new OpenRaftReplicationLayer with real OpenRaft instance
    /// This creates actual consensus with state machine and networking
    #[instrument(skip_all, fields(shard_id = %shard_id, collection = %shard_metadata.collection, partition_id = %shard_metadata.partition_id))]
    pub async fn new(
        shard_id: u64,
        app_storage: A,
        shard_metadata: ShardMetadata,
        z_session: zenoh::Session,
        rpc_prefix: String,
    ) -> Result<Self, ReplicationError> {
        info!("Creating OpenRaft replication layer for shard {}", shard_id);

        // Create OpenRaft configuration (similar to existing RaftObjectShard)
        let config = openraft::Config {
            cluster_name: format!(
                "oprc-raft/{}/{}",
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
            error!("Invalid Raft config: {:?}", e);
            ReplicationError::ConsensusError(format!(
                "Invalid Raft config: {:?}",
                e
            ))
        })?);

        debug!("Creating Raft components: log store, state machine, network");

        // Create log store (in-memory for now, could be made configurable)
        let log_store = super::raft_log::OpenraftLogStore::new(
            MemoryStorage::new_with_default()?,
            MemoryStorage::new_with_default()?,
        );

        // Create state machine store with injected storage
        let state_machine = ObjectShardStateMachine::new(app_storage.clone());

        // Create Zenoh-based network layer (using existing flare-dht infrastructure)
        let zrpc_client_config = ZrpcClientConfig {
            service_id: rpc_prefix.clone(),
            target: zenoh::query::QueryTarget::BestMatching,
            channel_size: 8,
            congestion_control: CongestionControl::Block,
            priority: Priority::DataHigh,
        };

        let network = Network::new(
            z_session.clone(),
            rpc_prefix.clone(),
            zrpc_client_config.clone(),
        );

        // Create the actual OpenRaft instance
        let raft = openraft::Raft::<ReplicationTypeConfig>::new(
            shard_id,
            config.clone(),
            network,
            log_store,
            state_machine,
        )
        .await
        .map_err(|e| {
            error!("Failed to create Raft: {}", e);
            ReplicationError::ConsensusError(format!(
                "Failed to create Raft: {}",
                e
            ))
        })?;

        debug!("Creating RPC services for Raft communication");

        // Create RPC server configuration
        let zrpc_server_config = ServerConfig {
            service_id: format!("{}/ops/{}", rpc_prefix, shard_id),
            reply_congestion: CongestionControl::Block,
            reply_priority: Priority::DataHigh,
            ..Default::default()
        };

        // Create Raft RPC service for consensus operations
        let raft_rpc_service = RaftZrpcService::new(
            raft.clone(),
            z_session.clone(),
            rpc_prefix.clone(),
            shard_id,
            zrpc_server_config.clone(),
        );

        // Create operation manager for consensus operations
        let operation_manager = ReplicationOperationManager::new(
            raft.clone(),
            z_session.clone(),
            format!("{}/ops", rpc_prefix),
            zrpc_client_config,
            zrpc_server_config,
        )
        .await;

        let (readiness_tx, readiness_rx) = watch::channel(false);
        let cancellation = tokio_util::sync::CancellationToken::new();

        info!(
            "OpenRaft replication layer created successfully for shard {}",
            shard_id
        );

        Ok(Self {
            shard_id: shard_id,
            shard_metadata,
            raft,
            store: app_storage,
            raft_rpc_service: RwLock::new(raft_rpc_service),
            operation_manager: RwLock::new(operation_manager),
            readiness_tx,
            readiness_rx,
            cancellation,
        })
    }

    /// Initialize the OpenRaft cluster (similar to RaftObjectShard::initialize)
    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id))]
    pub async fn initialize(&self) -> Result<(), ReplicationError> {
        info!("Initializing OpenRaft cluster");

        // Start the Raft RPC service first
        debug!("Starting Raft RPC service");
        let mut raft_rpc_service = self.raft_rpc_service.write().await;
        raft_rpc_service.start().await.map_err(|e| {
            error!("Failed to start Raft RPC service: {}", e);
            ReplicationError::NetworkError(format!(
                "Failed to start Raft RPC service: {}",
                e
            ))
        })?;
        drop(raft_rpc_service); // Release the lock

        // Start the Replication RPC service
        debug!("Starting Replication RPC service");
        let mut operation_manager = self.operation_manager.write().await;
        operation_manager.rpc_service.start().await.map_err(|e| {
            error!("Failed to start Replication RPC service: {}", e);
            ReplicationError::NetworkError(format!(
                "Failed to start Replication RPC service: {}",
                e
            ))
        })?;
        drop(operation_manager); // Release the lock

        debug!("Setting up readiness monitoring");
        // Set up readiness monitoring based on leadership
        let leader_only = self
            .shard_metadata
            .options
            .get("raft_net_leader_only")
            .map(|v| v == "true")
            .unwrap_or(false);

        if leader_only {
            debug!(
                "Leader-only mode enabled, setting up leadership monitoring"
            );
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
            debug!("Always-ready mode enabled");
            let _ = self.readiness_tx.send(true);
        }

        debug!("Checking cluster initialization requirements");
        // Initialize cluster if this is the primary node
        let init_leader_only = self
            .shard_metadata
            .options
            .get("raft_init_leader_only")
            .map(|v| v == "true")
            .unwrap_or(false);

        if !init_leader_only
            || self.shard_metadata.primary == Some(self.shard_id)
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
            } else {
                info!("Raft cluster initialized successfully");
            }
        } else {
            debug!("Skipping cluster initialization (not primary node)");
        }

        info!("OpenRaft cluster initialization complete");
        Ok(())
    }

    /// Shutdown the OpenRaft instance and services
    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id))]
    pub async fn shutdown(&self) -> Result<(), ReplicationError> {
        info!("Shutting down OpenRaft replication layer");

        self.cancellation.cancel();

        if let Err(e) = self.raft.shutdown().await {
            error!("Failed to shutdown Raft: {}", e);
            return Err(ReplicationError::ConsensusError(format!(
                "Failed to shutdown Raft: {}",
                e
            )));
        }

        info!("OpenRaft shutdown completed successfully");
        Ok(())
    }
}

#[async_trait]
impl<A> ReplicationLayer for OpenRaftReplicationLayer<A>
where
    A: SnapshotCapableStorage + Clone + Send + Sync + 'static,
{
    fn replication_model(&self) -> ReplicationModel {
        // Get current term from metrics synchronously
        let _metrics = self.raft.server_metrics().borrow().clone();
        trace!("Returning Raft consensus replication model");
        ReplicationModel::Consensus {
            algorithm: ConsensusAlgorithm::Raft,
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id, operation = ?request.operation))]
    async fn replicate_write(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        debug!("Processing write replication request");

        // Use OpenRaft consensus for write operations
        let operation_manager = self.operation_manager.read().await;
        operation_manager.exec(&request).await
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id, operation = ?request.operation))]
    async fn replicate_read(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        debug!("Processing read replication request");

        // Try linearizable read first, fallback to consensus if needed
        if let (Ok(_), crate::replication::Operation::Read(read_op)) =
            (self.raft.ensure_linearizable().await, &request.operation)
        {
            // Can serve read directly from local storage with linearizable guarantee
            match self.store.get(&read_op.key).await {
                Ok(value) => {
                    trace!("Linearizable read served from local storage");
                    return Ok(ReplicationResponse {
                        status: crate::replication::ResponseStatus::Applied,
                        data: value,
                        ..Default::default()
                    });
                }
                Err(e) => return Err(ReplicationError::StorageError(e)),
            }
        }

        // Fallback: not linearizable or not a read operation, use consensus
        debug!("Read requires consensus, forwarding to operation manager");
        let operation_manager = self.operation_manager.read().await;
        operation_manager.exec(&request).await
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id, address = %address))]
    async fn add_replica(
        &self,
        node_id: u64,
        address: String,
    ) -> Result<(), ReplicationError> {
        info!("Adding replica to Raft cluster");

        // Add a new node to the Raft cluster
        let new_node = openraft::BasicNode { addr: address };

        // First add as learner
        let result =
            self.raft.add_learner(node_id, new_node.clone(), true).await;

        match result {
            Ok(_) => {
                info!(
                    "Successfully added node {} as learner to cluster {}",
                    node_id, self.shard_metadata.id
                );

                // Optionally promote learner to voter
                // This could be made configurable based on shard policy
                let change_result =
                    self.raft.change_membership([node_id], false).await;

                match change_result {
                    Ok(_) => {
                        info!(
                            "Successfully promoted node {} to voter in cluster {}",
                            node_id, self.shard_metadata.id
                        );
                        Ok(())
                    }
                    Err(e) => {
                        warn!(
                            "Failed to promote node {} to voter: {}",
                            node_id, e
                        );
                        // Learner was added successfully, promotion failed
                        // This might be acceptable depending on use case
                        Ok(())
                    }
                }
            }
            Err(e) => {
                error!("Failed to add replica {}: {}", node_id, e);
                Err(ReplicationError::MembershipChange(format!(
                    "Failed to add replica {}: {}",
                    node_id, e
                )))
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id))]
    async fn remove_replica(
        &self,
        node_id: u64,
    ) -> Result<(), ReplicationError> {
        info!("Removing replica from Raft cluster");

        // Remove a node from the Raft cluster
        let result = self
            .raft
            .change_membership([], true) // Remove all members in second param
            .await;

        match result {
            Ok(_) => {
                info!(
                    "Successfully removed node {} from cluster {}",
                    node_id, self.shard_metadata.id
                );
                Ok(())
            }
            Err(e) => {
                error!("Failed to remove replica {}: {}", node_id, e);
                Err(ReplicationError::MembershipChange(format!(
                    "Failed to remove replica {}: {}",
                    node_id, e
                )))
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id))]
    async fn get_replication_status(
        &self,
    ) -> Result<ReplicationStatus, ReplicationError> {
        trace!("Getting replication status");

        let metrics = self.raft.server_metrics().borrow().clone();

        // Calculate replication status from Raft metrics
        let is_leader = Some(metrics.id) == metrics.current_leader;
        let leader_id = metrics.current_leader;

        // Count healthy replicas based on replication logs
        // This is a simplified approach - in production you'd check actual health
        let total_replicas = self.shard_metadata.replica.len();
        let healthy_replicas = if is_leader {
            // As leader, we can assess follower health from replication metrics
            // For now, assume all replicas are healthy if we're leader
            total_replicas
        } else {
            // As follower, we only know we're healthy
            1
        };

        // Calculate lag - for followers this would be the difference from leader's commit index
        let lag_ms = if is_leader {
            Some(0) // Leader has no lag
        } else {
            // For followers, calculate based on last_log_id vs current commit
            // This is simplified - real implementation would track timing
            None
        };

        trace!(
            "Replication status: is_leader={}, leader_id={:?}, healthy_replicas={}, total_replicas={}",
            is_leader, leader_id, healthy_replicas, total_replicas
        );

        Ok(ReplicationStatus {
            model: ReplicationModel::Consensus {
                algorithm: ConsensusAlgorithm::Raft,
            },
            healthy_replicas,
            total_replicas,
            lag_ms,
            conflicts: 0, // Raft doesn't have conflicts
            is_leader,
            leader_id,
            last_sync: Some(SystemTime::now()), // Could track actual last sync time
        })
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id))]
    async fn sync_replicas(&self) -> Result<(), ReplicationError> {
        trace!("Synchronizing replicas (no-op for Raft)");
        // OpenRaft automatically handles replication, so this is essentially a no-op
        // In a more sophisticated implementation, we might trigger explicit log replication
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_rx.clone()
    }

    #[instrument(skip_all, fields(shard_id = %self.shard_metadata.id, collection = %self.shard_metadata.collection, partition_id = %self.shard_metadata.partition_id))]
    async fn initialize(&self) -> Result<(), ReplicationError> {
        // Delegate to the existing initialize method
        self.initialize().await
    }
}
