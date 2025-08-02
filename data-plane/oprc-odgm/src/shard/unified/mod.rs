// Core modules
pub mod config;
pub mod object_shard;
pub mod traits;

// Re-export main types for convenience
pub use config::{ShardConfig, ShardError, ShardMetrics};
pub use object_shard::ObjectUnifiedShard; // ✅ Re-enabled after CompositeStorage refactoring
pub use traits::{
    ConsistencyConfig, ReplicationType, ShardMetadata, ShardState,
    ShardTransaction, StorageType, WriteConsistency,
};
