pub mod basic;
pub mod invocation;
mod liveliness;
// mod mst;
// mod raft;
pub mod unified;
pub use basic::ObjectData;
pub use basic::ObjectError;
pub use basic::ObjectVal;
pub use invocation::InvocationOffloader;

// Re-export the enhanced ShardMetadata from unified traits
pub use unified::traits::ShardMetadata;

// Migration helpers: re-export new unified types for easier transition
pub use unified::{
    ArcUnifiedObjectShard, HealthCheckResult, ManagerStats, ObjectUnifiedShard,
    UnifiedShardConfig, UnifiedShardFactory, UnifiedShardManager,
};

pub type ShardId = u64;
