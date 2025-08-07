// Core modules
pub mod config;
pub mod factory;
pub mod manager;
pub mod object_shard;
pub mod object_trait;
pub mod traits;

// Re-export main types for convenience
pub use config::{ShardConfig, ShardError, ShardMetrics};
pub use factory::UnifiedShardFactory;
pub use manager::{UnifiedShardManager, HealthCheckResult, ManagerStats};
pub use object_shard::ObjectUnifiedShard; // ✅ Re-enabled after CompositeStorage refactoring
pub use object_trait::{UnifiedObjectShard, UnifiedShardTransaction, BoxedUnifiedObjectShard, ArcUnifiedObjectShard, IntoUnifiedShard};
pub use traits::{
    ConsistencyConfig, ShardMetadata,
    ShardTransaction, WriteConsistency,
};
