pub mod basic;
pub mod builder;
pub mod config;
pub mod entry_store_impl;
pub mod invocation;
mod liveliness;
pub mod manager;
pub mod network;
pub mod object_api;
pub mod object_shard;
pub mod object_trait;
pub mod traits;
// mod mst;
// mod raft;

pub use basic::ObjectData;
pub use basic::ObjectError;
pub use basic::ObjectVal;
pub use invocation::InvocationOffloader;

// Re-export types from config and traits
pub use config::{ShardConfig, ShardError, ShardMetrics};
pub use traits::{
    ConsistencyConfig, ShardMetadata, ShardTransaction, WriteConsistency,
};

// Re-export builder types
pub use builder::{
    BasicShard, EventPipelineConfig, MstShard, RaftShard, ShardBuilder,
    ShardConstructors, ShardOptions, create_shard_dynamic,
    create_shard_with_pool,
};

// Backward compatibility aliases (deprecated)
#[allow(deprecated)]
pub use builder::UnifiedShardFactory;
#[deprecated(since = "0.1.0", note = "Use ShardOptions instead")]
pub type UnifiedShardConfig = builder::ShardOptions;

pub use manager::{HealthCheckResult, ManagerStats, UnifiedShardManager};

// Re-export types from object_trait
pub use object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard,
    ObjectShard, UnifiedShardTransaction,
};

// Re-export types from object_shard and network
pub use network::UnifiedShardNetwork;
pub use object_shard::ObjectUnifiedShard;

pub type ShardId = u64;
