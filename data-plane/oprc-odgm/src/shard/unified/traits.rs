use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::watch;

use crate::replication::ReadConsistency;
use oprc_dp_storage::{ApplicationDataStorage, RaftLogStorage, StorageValue};

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
    type LogStorage: RaftLogStorage;
    type AppStorage: ApplicationDataStorage; // Can optionally implement SnapshotCapableStorage

    /// Metadata and configuration
    fn meta(&self) -> &ShardMetadata;
    fn storage_type(&self) -> StorageType;
    fn replication_type(&self) -> ReplicationType;

    /// Storage layer access
    /// Log storage is only available for Raft consensus
    fn get_log_storage(&self) -> Option<&Self::LogStorage>;
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

    /// Storage-specific operations
    async fn create_snapshot(&self) -> Result<String, Self::Error>;
    async fn restore_from_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<(), Self::Error>;
    async fn compact_storage(&self) -> Result<(), Self::Error>;

    /// Existing merge operation preserved for compatibility
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

// ============================================================================
// Composite Storage for Enhanced Storage Architecture
// ============================================================================

/// Storage abstraction that can work with or without log storage
pub enum FlexibleStorage<L, A>
where
    L: RaftLogStorage,
    A: ApplicationDataStorage,
{
    /// Raft consensus - needs both log and application storage
    RaftStorage { log_storage: L, app_storage: A },
    /// Non-Raft replication - only needs application storage  
    AppOnlyStorage { app_storage: A },
}

impl<L, A> FlexibleStorage<L, A>
where
    L: RaftLogStorage,
    A: ApplicationDataStorage,
{
    /// Create Raft storage with both log and application layers
    pub fn new_raft_storage(log_storage: L, app_storage: A) -> Self {
        Self::RaftStorage {
            log_storage,
            app_storage,
        }
    }

    /// Create app-only storage for non-Raft replication
    pub fn new_app_only_storage(app_storage: A) -> Self {
        Self::AppOnlyStorage { app_storage }
    }

    /// Access to application storage layer (always available)
    pub fn get_app_storage(&self) -> &A {
        match self {
            Self::RaftStorage { app_storage, .. } => app_storage,
            Self::AppOnlyStorage { app_storage } => app_storage,
        }
    }

    /// Access to log storage layer (only available for Raft)
    pub fn get_log_storage(&self) -> Option<&L> {
        match self {
            Self::RaftStorage { log_storage, .. } => Some(log_storage),
            Self::AppOnlyStorage { .. } => None,
        }
    }

    /// Check if this storage supports Raft consensus
    pub fn has_log_storage(&self) -> bool {
        matches!(self, Self::RaftStorage { .. })
    }

    /// Create coordinated snapshot (only for Raft storage)
    pub async fn create_coordinated_snapshot(
        &self,
        last_applied_index: u64,
    ) -> Result<String, oprc_dp_storage::SpecializedStorageError> {
        match self {
            Self::RaftStorage { app_storage, .. } => {
                // Export application data
                let app_data = app_storage.export_all().await.map_err(|e| {
                    oprc_dp_storage::SpecializedStorageError::ApplicationData(
                        e.to_string(),
                    )
                })?;

                // Create snapshot and store in application storage with special key
                let snapshot_id = uuid::Uuid::new_v4().to_string();
                let snapshot_data = bincode::serde::encode_to_vec(
                    &app_data,
                    bincode::config::standard(),
                )
                .map_err(|e| {
                    oprc_dp_storage::SpecializedStorageError::ApplicationData(
                        format!("Failed to serialize snapshot data: {}", e),
                    )
                })?;

                let snapshot_key = format!("__raft_snapshot_{}", snapshot_id);
                app_storage
                    .put(
                        snapshot_key.as_bytes(),
                        StorageValue::from(snapshot_data),
                    )
                    .await
                    .map_err(|e| {
                        oprc_dp_storage::SpecializedStorageError::ApplicationData(
                            format!("Failed to store snapshot: {}", e),
                        )
                    })?;

                Ok(snapshot_id)
            }
            Self::AppOnlyStorage { .. } => {
                Err(oprc_dp_storage::SpecializedStorageError::ApplicationData(
                    "Coordinated snapshots not supported for non-Raft storage"
                        .to_string(),
                ))
            }
        }
    }

    /// Create application state snapshot (works for all storage types)
    pub async fn create_app_state_snapshot(
        &self,
    ) -> Result<String, oprc_dp_storage::SpecializedStorageError> {
        let app_storage = self.get_app_storage();

        // Export application data
        let app_data = app_storage.export_all().await.map_err(|e| {
            oprc_dp_storage::SpecializedStorageError::ApplicationData(
                e.to_string(),
            )
        })?;

        // Create snapshot and store in application storage
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let snapshot_data = bincode::serde::encode_to_vec(
            &app_data,
            bincode::config::standard(),
        )
        .map_err(|e| {
            oprc_dp_storage::SpecializedStorageError::ApplicationData(format!(
                "Failed to serialize snapshot data: {}",
                e
            ))
        })?;

        let snapshot_key = format!("__snapshot_{}", snapshot_id);
        app_storage
            .put(snapshot_key.as_bytes(), StorageValue::from(snapshot_data))
            .await
            .map_err(|e| {
                oprc_dp_storage::SpecializedStorageError::ApplicationData(
                    format!("Failed to store snapshot: {}", e),
                )
            })?;

        Ok(snapshot_id)
    }
}
