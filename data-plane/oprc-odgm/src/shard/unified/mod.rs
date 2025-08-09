// Core modules
pub mod config;
pub mod factory;
pub mod manager;
pub mod network;
pub mod object_shard;
pub mod object_trait;
pub mod traits;

// Re-export main types for convenience
pub use config::{ShardConfig, ShardError, ShardMetrics};
pub use factory::UnifiedShardFactory;
pub use manager::{HealthCheckResult, ManagerStats, UnifiedShardManager};
pub use network::UnifiedShardNetwork;
pub use object_shard::ObjectUnifiedShard; // ✅ Re-enabled after CompositeStorage refactoring
pub use object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard,
    ObjectShard, UnifiedShardTransaction,
};
pub use traits::{
    ConsistencyConfig, ShardMetadata, ShardTransaction, WriteConsistency,
};
