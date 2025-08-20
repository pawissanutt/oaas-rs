use crate::shard::ShardId;

/// Shard metrics for monitoring
#[derive(Debug)]
pub struct ShardMetrics {
    pub collection: String,
    pub partition_id: u16,
    pub operations_count: std::sync::atomic::AtomicU64,
    pub errors_count: std::sync::atomic::AtomicU64,
}

impl ShardMetrics {
    pub fn new(collection: &str, partition_id: u16) -> Self {
        Self {
            collection: collection.to_string(),
            partition_id,
            operations_count: std::sync::atomic::AtomicU64::new(0),
            errors_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

/// Shard configuration
#[derive(Debug, Clone)]
pub struct ShardConfig {
    pub batch_size: usize,
    pub timeout_ms: u64,
    pub enable_metrics: bool,
}

impl ShardConfig {
    pub fn from_metadata(metadata: &super::traits::ShardMetadata) -> Self {
        Self {
            batch_size: metadata
                .options
                .get("batch_size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
            timeout_ms: metadata
                .options
                .get("timeout_ms")
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000),
            enable_metrics: metadata
                .options
                .get("enable_metrics")
                .map(|s| s == "true")
                .unwrap_or(true),
        }
    }
}

/// Unified shard errors
#[derive(Debug, thiserror::Error)]
pub enum ShardError {
    #[error("Storage error: {0}")]
    StorageError(#[from] oprc_dp_storage::StorageError),

    #[error("Replication error: {0}")]
    ReplicationError(#[from] crate::replication::ReplicationError),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("No shards found: {0:?}")]
    NoShardsFound(Vec<ShardId>),

    #[error("Invalid key format")]
    InvalidKey,

    #[error("Not ready")]
    NotReady,

    #[error("Not leader")]
    NotLeader,

    #[error("ODGM error: {0}")]
    OdgmError(#[from] crate::error::OdgmError),
}
