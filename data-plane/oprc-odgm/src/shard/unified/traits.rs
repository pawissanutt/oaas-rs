use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use crate::replication::ReadConsistency;

/// Enhanced shard trait that supports pluggable storage and replication
#[async_trait]
pub trait ShardState: Send + Sync {
    type Key: Send + Clone + Serialize + for<'de> Deserialize<'de>;
    type Entry: Send
        + Sync
        + Default
        + Clone
        + Serialize
        + for<'de> Deserialize<'de>;
    type Error: Error + Send + Sync + 'static;

    /// Get shard metadata
    fn meta(&self) -> &ShardMetadata;

    /// Get storage backend type
    fn storage_type(&self) -> StorageType;

    /// Get replication type
    fn replication_type(&self) -> ReplicationType;

    /// Initialize the shard
    async fn initialize(&self) -> Result<(), Self::Error>;

    /// Close the shard
    async fn close(&mut self) -> Result<(), Self::Error>;

    // Core operations (enhanced from existing ShardState)
    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, Self::Error>;
    async fn set(
        &self,
        key: Self::Key,
        entry: Self::Entry,
    ) -> Result<(), Self::Error>;
    async fn delete(&self, key: &Self::Key) -> Result<(), Self::Error>;
    async fn count(&self) -> Result<u64, Self::Error>;

    // Enhanced operations (new)
    async fn scan(
        &self,
        prefix: Option<&Self::Key>,
    ) -> Result<Vec<(Self::Key, Self::Entry)>, Self::Error>;
    async fn batch_set(
        &self,
        entries: Vec<(Self::Key, Self::Entry)>,
    ) -> Result<(), Self::Error>;
    async fn batch_delete(
        &self,
        keys: Vec<Self::Key>,
    ) -> Result<(), Self::Error>;

    // Transaction support
    async fn begin_transaction(
        &self,
    ) -> Result<
        Box<
            dyn ShardTransaction<
                Key = Self::Key,
                Entry = Self::Entry,
                Error = Self::Error,
            >,
        >,
        Self::Error,
    >;

    // Existing merge operation preserved for compatibility
    async fn merge(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<Self::Entry, Self::Error> {
        self.set(key.clone(), value).await?;
        let item = self.get(&key).await?;
        match item {
            Some(entry) => Ok(entry),
            None => Ok(Self::Entry::default()),
        }
    }

    // Existing readiness watching preserved
    fn watch_readiness(&self) -> watch::Receiver<bool>;
}

/// Transaction trait for atomic operations across storage and replication
#[async_trait]
pub trait ShardTransaction: Send + Sync {
    type Key: Send + Clone;
    type Entry: Send + Sync;
    type Error: Error + Send + Sync + 'static;

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, Self::Error>;
    async fn set(
        &mut self,
        key: Self::Key,
        entry: Self::Entry,
    ) -> Result<(), Self::Error>;
    async fn delete(&mut self, key: &Self::Key) -> Result<(), Self::Error>;
    async fn commit(self: Box<Self>) -> Result<(), Self::Error>;
    async fn rollback(self: Box<Self>) -> Result<(), Self::Error>;
}

/// Shard metadata with enhanced configuration - extends the existing ShardMetadata
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShardMetadata {
    pub id: u64,
    pub collection: String,
    pub partition_id: u16,
    pub owner: Option<u64>,
    pub primary: Option<u64>,
    pub replica: Vec<u64>,
    pub replica_owner: Vec<u64>,
    pub shard_type: String,
    pub options: std::collections::HashMap<String, String>,

    // Compatibility with existing ShardMetadata
    pub invocations: Option<oprc_pb::InvocationRoute>,

    // New configuration fields
    pub storage_config: Option<oprc_dp_storage::StorageConfig>,
    pub replication_config: Option<crate::replication::BasicReplicationConfig>,
    pub consistency_config: Option<ConsistencyConfig>,
}

impl From<crate::shard::ShardMetadata> for ShardMetadata {
    fn from(existing: crate::shard::ShardMetadata) -> Self {
        Self {
            id: existing.id,
            collection: existing.collection,
            partition_id: existing.partition_id,
            owner: existing.owner,
            primary: existing.primary,
            replica: existing.replica,
            replica_owner: existing.replica_owner,
            shard_type: existing.shard_type,
            options: existing.options,
            invocations: Some(existing.invocations),
            storage_config: None,
            replication_config: None,
            consistency_config: None,
        }
    }
}

impl Into<crate::shard::ShardMetadata> for ShardMetadata {
    fn into(self) -> crate::shard::ShardMetadata {
        crate::shard::ShardMetadata {
            id: self.id,
            collection: self.collection,
            partition_id: self.partition_id,
            owner: self.owner,
            primary: self.primary,
            replica: self.replica,
            replica_owner: self.replica_owner,
            shard_type: self.shard_type,
            options: self.options,
            invocations: self.invocations.unwrap_or_default(),
        }
    }
}

impl ShardMetadata {
    /// Helper method to determine storage backend from metadata
    pub fn storage_backend_type(&self) -> oprc_dp_storage::StorageBackendType {
        self.storage_config
            .as_ref()
            .map(|c| c.backend_type.clone())
            .unwrap_or_else(|| {
                // Fallback to parsing from shard_type or options
                match self.shard_type.as_str() {
                    "memory" => oprc_dp_storage::StorageBackendType::Memory,
                    "redb" => oprc_dp_storage::StorageBackendType::Redb,
                    "fjall" => oprc_dp_storage::StorageBackendType::Fjall,
                    "rocksdb" => oprc_dp_storage::StorageBackendType::RocksDb,
                    _ => oprc_dp_storage::StorageBackendType::Memory,
                }
            })
    }

    /// Helper method to determine replication type
    pub fn replication_type(&self) -> ReplicationType {
        if let Some(_repl_config) = &self.replication_config {
            return ReplicationType::EventualConsistency;
        }

        // Infer from existing fields
        if !self.replica.is_empty() {
            if self.shard_type.contains("mst") {
                ReplicationType::Mst
            } else if self.shard_type.contains("raft") {
                ReplicationType::Raft
            } else {
                ReplicationType::EventualConsistency
            }
        } else {
            ReplicationType::None
        }
    }
}

/// Storage backend types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageType {
    Memory,
    Persistent,
    Hybrid,
}

/// Replication types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationType {
    None,
    Raft,
    Mst,
    EventualConsistency,
}

/// Consistency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyConfig {
    pub read_consistency: ReadConsistency,
    pub write_consistency: WriteConsistency,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteConsistency {
    Async,
    Sync,
    Majority,
    All,
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            read_consistency: ReadConsistency::Sequential,
            write_consistency: WriteConsistency::Sync,
            timeout_ms: 5000,
        }
    }
}
