// Core modules
pub mod config;
pub mod core;
pub mod object_shard;
pub mod traits;

// Re-export main types for convenience
pub use config::{ShardConfig, ShardError, ShardMetrics};
pub use core::{UnifiedShard, UnifiedShardTransaction};
pub use object_shard::ObjectUnifiedShard;
pub use traits::{
    ConsistencyConfig, ReplicationType, ShardMetadata, ShardState,
    ShardTransaction, StorageType, WriteConsistency,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::NoReplication;
    use oprc_dp_storage::{MemoryStorage, StorageConfig};

    #[tokio::test]
    async fn test_shard_metadata_config_parsing() {
        let mut metadata = ShardMetadata::default();
        metadata.shard_type = "memory".to_string();

        assert_eq!(
            metadata.storage_backend_type(),
            oprc_dp_storage::StorageBackendType::Memory
        );
        assert_eq!(metadata.replication_type(), ReplicationType::None);

        metadata.replica = vec![1, 2, 3];
        metadata.shard_type = "raft".to_string();
        assert_eq!(metadata.replication_type(), ReplicationType::Raft);
    }

    #[tokio::test]
    async fn test_unified_shard_creation() {
        let metadata = ShardMetadata {
            id: 1,
            collection: "test".to_string(),
            partition_id: 0,
            ..Default::default()
        };

        let storage_config = StorageConfig::memory();
        let storage = MemoryStorage::new(storage_config).await.unwrap();
        let replication = Some(NoReplication);

        let shard = UnifiedShard::new(metadata, storage, replication).await;
        assert!(shard.is_ok());

        let shard = shard.unwrap();
        assert_eq!(shard.meta().id, 1);
        assert_eq!(shard.meta().collection, "test");
    }
}
