use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use oprc_dp_storage::ApplicationDataStorage;

/// Enhanced shard trait that supports pluggable storage and replication
/// Updated to match FINAL_INTEGRATION_DESIGN.md specification
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

    /// Storage layer access types
    type AppStorage: ApplicationDataStorage; // Can optionally implement SnapshotCapableStorage

    /// Metadata and configuration
    fn meta(&self) -> &ShardMetadata;
    fn storage_type(&self) -> StorageType;
    fn replication_type(&self) -> ReplicationType;

    /// Storage layer access
    fn get_app_storage(&self) -> &Self::AppStorage;

    /// Lifecycle management
    async fn initialize(&self) -> Result<(), Self::Error>;
    async fn close(&mut self) -> Result<(), Self::Error>;
    fn watch_readiness(&self) -> watch::Receiver<bool>;

    /// Core data operations (work through replication layer)
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

    /// Enhanced operations
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

    /// Transaction support
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
    pub invocations: oprc_pb::InvocationRoute,

    // New configuration fields
    pub storage_config: Option<oprc_dp_storage::StorageConfig>,
    pub replication_config: Option<crate::replication::BasicReplicationConfig>,
    pub consistency_config: Option<ConsistencyConfig>,
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
pub enum ReadConsistency {
    Linearizable,
    ReadYourWrite,
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
            read_consistency: ReadConsistency::ReadYourWrite,
            write_consistency: WriteConsistency::Sync,
            timeout_ms: 5000,
        }
    }
}
