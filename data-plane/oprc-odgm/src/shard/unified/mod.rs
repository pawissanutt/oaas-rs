// Backward compatibility re-exports
// All modules have been moved to parent shard module

// Re-export submodules for backward compatibility with paths like shard::unified::traits::*
pub use super::config;
pub use super::factory;
pub use super::traits;

// Re-export from parent shard module
pub use super::config::{ShardConfig, ShardError, ShardMetrics};
pub use super::entry_store_impl;
pub use super::factory::{UnifiedShardConfig, UnifiedShardFactory};
pub use super::manager::{
    HealthCheckResult, ManagerStats, UnifiedShardManager,
};
pub use super::network;
pub use super::network::UnifiedShardNetwork;
pub use super::object_shard;
pub use super::object_shard::ObjectUnifiedShard;
pub use super::object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard,
    ObjectShard, UnifiedShardTransaction,
};
pub use super::traits::{
    ConsistencyConfig, ShardMetadata, ShardTransaction, WriteConsistency,
};
