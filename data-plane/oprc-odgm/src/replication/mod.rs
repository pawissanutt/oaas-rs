use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use oprc_dp_storage::{StorageError, StorageValue};

pub mod mst;
pub mod no_replication;
pub mod raft;

// Re-export for convenience
pub use no_replication::NoReplication;

/// Core replication layer trait that abstracts different replication models
#[async_trait]
pub trait ReplicationLayer: Send + Sync {
    /// Get the replication model type
    fn replication_model(&self) -> ReplicationModel;

    /// Execute a write operation (handles consensus internally)
    async fn replicate_write(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError>;

    /// Execute a read operation (may be local or require coordination)
    async fn replicate_read(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError>;

    /// Add a new replica/member to the replication group
    async fn add_replica(
        &self,
        node_id: u64,
        address: String,
    ) -> Result<(), ReplicationError>;

    /// Remove a replica/member from the replication group
    async fn remove_replica(
        &self,
        node_id: u64,
    ) -> Result<(), ReplicationError>;

    /// Get current replication status/health
    async fn get_replication_status(
        &self,
    ) -> Result<ReplicationStatus, ReplicationError>;

    /// Sync with other replicas (for eventual consistency models)
    async fn sync_replicas(&self) -> Result<(), ReplicationError>;

    /// Get a watch receiver for readiness status
    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool>;

    /// Initialize the replication layer
    /// This should be called before using the replication layer for operations
    async fn initialize(&self) -> Result<(), ReplicationError>;
}

/// Replication models supported by the system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReplicationModel {
    /// Leader-follower consensus (Raft, PBFT, etc.)
    Consensus { algorithm: ConsensusAlgorithm },
    /// Conflict-free replicated data types (CRDTs, MST, etc.)
    ConflictFree { merge_strategy: MergeStrategy },
    /// Eventually consistent replication
    EventualConsistency {
        sync_interval: Duration,
        consistency_level: ConsistencyLevel,
    },
    /// No replication (single node)
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConsensusAlgorithm {
    Raft,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MergeStrategy {
    AutoMerge,      // Automatic conflict resolution
    LastWriterWins, // Timestamp-based resolution
    Manual,         // Manual conflict resolution required
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConsistencyLevel {
    Strong,     // Linearizable
    Sequential, // Sequential consistency
    Eventual,   // Eventually consistent
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReadConsistency {
    Linearizable, // Must confirm with leader
    Sequential,   // Leader preference but allow follower reads
    Eventual,     // Allow any replica reads
}

/// Request structure for replication operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRequest {
    pub operation: Operation,
    pub timestamp: SystemTime,
    pub source_node: u64,
}

impl ShardRequest {
    pub fn from_operation(operation: Operation, source_node: u64) -> Self {
        Self {
            operation,
            timestamp: SystemTime::now(),
            source_node,
        }
    }
}

/// Operations that can be replicated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Operation {
    Write(WriteOperation),
    Read(ReadOperation),
    Delete(DeleteOperation),
    Batch(Vec<Operation>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WriteOperation {
    pub key: StorageValue,
    pub value: StorageValue,
    pub ttl: Option<Duration>,
    pub return_old: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadOperation {
    pub key: StorageValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteOperation {
    pub key: StorageValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum OperationExtra {
    #[default]
    None,
    Write(bool), // Indicates if the write operation override the existing value
}

/// Response from replication operations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicationResponse {
    pub status: ResponseStatus,
    pub data: Option<StorageValue>,
    pub extra: OperationExtra,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ResponseStatus {
    #[default]
    Applied, // Operation successfully applied
    NotLeader {
        leader_hint: Option<u64>,
    }, // Not leader, hint for actual leader
    Failed(String),   // Operation failed with reason
    Conflict(String), // Conflict detected (for conflict-free replication)
    Retry,            // Temporary failure, retry recommended
}

/// Replication status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    pub model: ReplicationModel,
    pub healthy_replicas: usize,
    pub total_replicas: usize,
    pub lag_ms: Option<u64>,
    pub conflicts: usize,
    pub is_leader: bool,
    pub leader_id: Option<u64>,
    pub last_sync: Option<SystemTime>,
}

/// Configuration for different replication models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftReplicationConfig {
    pub heartbeat_interval_ms: u64,
    pub election_timeout_min_ms: u64,
    pub election_timeout_max_ms: u64,
    pub snapshot_threshold: u64,
    pub max_append_entries: usize,
}

impl Default for RaftReplicationConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 1000,
            election_timeout_min_ms: 1500,
            election_timeout_max_ms: 3000,
            snapshot_threshold: 10000,
            max_append_entries: 1000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MstReplicationConfig {
    pub mst_config: MstConfig,
    pub peer_addresses: Vec<String>,
    pub sync_interval: Duration,
    pub max_delta_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MstConfig {
    pub max_tree_depth: usize,
    pub hash_algorithm: HashAlgorithm,
    pub compression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HashAlgorithm {
    Sha256,
    Blake3,
    Xxhash,
}

impl Default for MstConfig {
    fn default() -> Self {
        Self {
            max_tree_depth: 32,
            hash_algorithm: HashAlgorithm::Blake3,
            compression: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicReplicationConfig {
    pub sync_interval: Duration,
    pub consistency_level: ConsistencyLevel,
    pub max_retry_attempts: u32,
    pub peer_addresses: Vec<String>,
}

impl Default for BasicReplicationConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(30),
            consistency_level: ConsistencyLevel::Eventual,
            max_retry_attempts: 3,
            peer_addresses: Vec::new(),
        }
    }
}

/// Replication errors
#[derive(Debug, thiserror::Error)]
pub enum ReplicationError {
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Consensus error: {0}")]
    ConsensusError(String),

    #[error("Membership change error: {0}")]
    MembershipChange(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Conflict: {0}")]
    Conflict(String),
}
