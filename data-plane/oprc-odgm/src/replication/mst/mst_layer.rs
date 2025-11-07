//! Main MST replication layer implementation

use async_trait::async_trait;
use merkle_search_tree::MerkleSearchTree;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::instrument;

use crate::replication::{
    ReplicationError, ReplicationLayer, ReplicationModel, ReplicationResponse,
};
use crate::shard::ShardMetadata;
use oprc_dp_storage::{StorageBackend, StorageResult, StorageValue};

use super::mst_network::ZenohMstNetworking;
use super::mst_traits::MstNetworking;
use super::types::{GenericNetworkPage, MstConfig, MstKey};

/// MST-based replication layer that works with any data type and StorageBackend
pub struct MstReplicationLayer<
    S: StorageBackend + 'static,
    T: Clone
        + Send
        + Sync
        + std::hash::Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + 'static,
> {
    storage: Arc<S>,
    mst: Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
    shard_id: u64,
    metadata: ShardMetadata,
    config: Arc<MstConfig<T>>,
    networking: Arc<ZenohMstNetworking<T, S>>,
    readiness_sender: tokio::sync::watch::Sender<bool>,
    readiness_receiver: tokio::sync::watch::Receiver<bool>,
}

impl<
    S: StorageBackend + 'static,
    T: Clone
        + Send
        + Sync
        + std::hash::Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + 'static,
> MstReplicationLayer<S, T>
{
    /// Create a new MST replication layer
    #[instrument(skip_all, fields(shard_id = %metadata.id, collection = %metadata.collection, partition_id = %metadata.partition_id))]
    pub fn new(
        storage: S,
        _shard_id: u64,
        metadata: ShardMetadata,
        config: MstConfig<T>,
        zenoh_session: zenoh::Session,
    ) -> Self {
        tracing::debug!("Creating new MST replication layer");

        let shard_id = metadata.id;
        let storage = Arc::new(storage);
        let mst = Arc::new(RwLock::new(MerkleSearchTree::default()));
        let config = Arc::new(config);

        // Create networking with access to storage, MST, and config
        let networking = Arc::new(ZenohMstNetworking::new(
            shard_id,
            format!("oprc/{}/{}", metadata.collection, metadata.partition_id),
            zenoh_session,
            storage.clone(),
            mst.clone(),
            config.clone(),
            shard_id,
        ));

        let (tx, rx) = tokio::sync::watch::channel(false);
        let layer = Self {
            storage,
            mst,
            shard_id,
            metadata,
            config,
            networking,
            readiness_sender: tx,
            readiness_receiver: rx,
        };

        tracing::debug!("MST replication layer created successfully");

        layer
    }

    /// Initialize the MST from existing storage data
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id))]
    pub async fn initialize(&self) -> StorageResult<()> {
        tracing::info!("Initializing MST replication layer");

        // Rebuild MST from persistent storage
        tracing::debug!("Starting MST rebuild from storage");
        self.rebuild_mst_from_storage().await?;

        // Always start networking layer (including in test mode for debugging)
        tracing::debug!("Starting networking layer");
        self.networking.start().await.map_err(|e| {
            tracing::error!("Failed to start networking layer: {}", e);
            oprc_dp_storage::StorageError::serialization(&e.to_string())
        })?;

        self.start_periodic_publication().await;

        // Signal readiness
        let _ = self.readiness_sender.send(true);
        tracing::info!("MST replication layer initialization complete");

        Ok(())
    }

    /// Start periodic MST page publication
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id))]
    async fn start_periodic_publication(&self) {
        let interval_ms: u64 = self
            .metadata
            .options
            .get("mst_sync_interval")
            .unwrap_or(&"5000".to_string())
            .parse()
            .unwrap_or(5000);

        tracing::debug!(
            "Starting periodic MST publication with interval {}ms",
            interval_ms
        );

        let mst = self.mst.clone();
        let networking = self.networking.clone();
        let node_id = self.shard_id;
        let shard_id = self.metadata.id;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_millis(interval_ms),
            );
            let mut publication_count = 0u64;

            loop {
                interval.tick().await;
                publication_count += 1;

                let mut mst_guard = mst.write().await;
                let root_hash = mst_guard.root_hash();

                tracing::trace!(
                    "MST root hash calculated: {:?} (shard_id={})",
                    root_hash,
                    shard_id
                );

                // Extract page ranges in a way that doesn't borrow
                let ranges_result = mst_guard.serialise_page_ranges();

                // Get MST pages and publish them
                let pages = {
                    let pages_vec = match ranges_result {
                        Some(ranges_slice) => ranges_slice.to_vec(),
                        None => vec![],
                    };
                    pages_vec
                };

                if !pages.is_empty() {
                    tracing::debug!(
                        "Publishing {} MST pages for node {} shard {} (publication #{})",
                        pages.len(),
                        node_id,
                        shard_id,
                        publication_count
                    );

                    let network_pages =
                        GenericNetworkPage::from_page_ranges(pages);

                    if let Err(err) =
                        networking.publish_pages(node_id, network_pages).await
                    {
                        tracing::error!(
                            "Failed to publish MST pages for node {} shard {}: {}",
                            node_id,
                            shard_id,
                            err
                        );
                    } else {
                        tracing::trace!(
                            "Successfully published MST pages for node {} shard {} (publication #{})",
                            node_id,
                            shard_id,
                            publication_count
                        );
                    }
                } else {
                    tracing::trace!(
                        "No MST pages to publish for node {} shard {} (publication #{})",
                        node_id,
                        shard_id,
                        publication_count
                    );
                }
            }
        });

        tracing::debug!(
            "Periodic MST publication task spawned for node {} shard {}",
            self.shard_id,
            self.metadata.id
        );
    }

    /// Rebuild the MST from all data in storage
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id))]
    pub async fn rebuild_mst_from_storage(&self) -> StorageResult<()> {
        tracing::info!("Rebuilding MST from storage");

        // Scan all entries from storage
        let entries = self.storage.scan(&[]).await?;
        let entry_count = entries.len();
        let mut mst = self.mst.write().await;

        for (key_bytes, value_bytes) in &entries {
            tracing::trace!(
                "Rebuilding MST entry: key_len={}, value_size={} bytes",
                key_bytes.len(),
                value_bytes.len()
            );
            if let Ok(entry) = (self.config.deserialize)(value_bytes.as_slice())
            {
                mst.upsert(MstKey(key_bytes.as_slice().to_vec()), &entry);
            } else {
                tracing::warn!(
                    "Failed to deserialize value during MST rebuild (key len {})",
                    key_bytes.len()
                );
            }
        }

        tracing::info!("MST rebuild complete, {} entries loaded", entry_count);
        Ok(())
    }

    /// Get an entry (reads from storage, not MST)
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id, key))]
    pub async fn get_raw(&self, key: &[u8]) -> StorageResult<Option<T>> {
        tracing::trace!("MST get operation");
        match self.storage.get(key).await? {
            Some(value_bytes) => {
                tracing::trace!(
                    "MST get found value: size={} bytes",
                    value_bytes.len()
                );
                let entry = (self.config.deserialize)(value_bytes.as_slice())?;
                Ok(Some(entry))
            }
            None => {
                tracing::trace!("MST get found no value");
                Ok(None)
            }
        }
    }

    /// Set an entry with LWW conflict resolution
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id, key))]
    pub async fn set_raw(&self, key: &[u8], entry: T) -> StorageResult<()> {
        tracing::debug!("MST set operation");
        let final_entry =
            if let Some(existing_bytes) = self.storage.get(key).await? {
                tracing::trace!(
                    "MST set found existing entry, applying merge function"
                );
                let existing =
                    (self.config.deserialize)(existing_bytes.as_slice())?;

                // Apply configurable merge function with LWW logic
                (self.config.merge_function)(existing, entry, self.shard_id)
            } else {
                tracing::trace!("MST set creating new entry");
                entry
            };

        // Serialize using configured function
        let value_bytes = (self.config.serialize)(&final_entry)?;
        let value_size = value_bytes.len();

        self.storage
            .put(key, StorageValue::from(value_bytes))
            .await?;

        // Update MST
        let mut mst = self.mst.write().await;
        mst.upsert(MstKey(key.to_vec()), &final_entry);

        tracing::debug!("MST set completed: value_size={} bytes", value_size);

        Ok(())
    }

    /// Set an entry with LWW conflict resolution
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id, key))]
    pub async fn set_with_return_old_raw(
        &self,
        key: &[u8],
        entry: T,
    ) -> StorageResult<Option<StorageValue>> {
        tracing::debug!("MST set_with_return_old operation");
        let mut old_value = None;
        let final_entry = if let Some(existing_bytes) =
            self.storage.get(key).await?
        {
            tracing::trace!(
                "MST set_with_return_old found existing entry, size={} bytes",
                existing_bytes.len()
            );
            let existing =
                (self.config.deserialize)(existing_bytes.as_slice())?;
            old_value = Some(existing_bytes);
            // Apply configurable merge function with LWW logic
            (self.config.merge_function)(existing, entry, self.shard_id)
        } else {
            tracing::trace!("MST set_with_return_old creating new entry");
            entry
        };

        // Serialize using configured function
        let value_bytes = (self.config.serialize)(&final_entry)?;
        let value_size = value_bytes.len();

        self.storage
            .put(key, StorageValue::from(value_bytes))
            .await?;

        // Update MST
        let mut mst = self.mst.write().await;
        mst.upsert(MstKey(key.to_vec()), &final_entry);

        tracing::debug!(
            "MST set_with_return_old completed: new_value_size={} bytes, had_old_value={}",
            value_size,
            old_value.is_some()
        );

        Ok(old_value)
    }

    /// Delete an entry
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id, key))]
    pub async fn delete_raw(&self, key: &[u8]) -> StorageResult<()> {
        tracing::debug!("MST delete operation");
        self.storage.delete(key).await?;

        // Remove from MST
        let _mst = self.mst.write().await;
        // MST doesn't have direct removal - we rebuild or use update with None
        // For now, we'll track deletions by updating with a tombstone or rebuilding
        // This is a limitation of the current MST library
        tracing::debug!(
            "Deleted key from storage, MST will be rebuilt on next restart"
        );

        Ok(())
    }

    // ---------------------------------------------------------------------
    // Convenience APIs (legacy numeric-key interface) retained for tests
    // ---------------------------------------------------------------------
    /// Convenience: set using a u64 key (big-endian) - used by existing tests
    #[allow(dead_code)]
    pub async fn set(&self, key: u64, entry: T) -> StorageResult<()> {
        self.set_raw(&key.to_be_bytes(), entry).await
    }

    /// Convenience: get using a u64 key
    #[allow(dead_code)]
    pub async fn get(&self, key: u64) -> StorageResult<Option<T>> {
        self.get_raw(&key.to_be_bytes()).await
    }

    /// Convenience: delete using a u64 key
    #[allow(dead_code)]
    pub async fn delete(&self, key: u64) -> StorageResult<()> {
        self.delete_raw(&key.to_be_bytes()).await
    }

    /// Get the current MST root hash for synchronization
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id))]
    pub async fn get_root_hash(&self) -> Option<Vec<u8>> {
        let mut mst = self.mst.write().await;
        let root_hash = mst.root_hash();
        // Convert RootHash to bytes
        Some(root_hash.as_ref().to_vec())
    }

    /// Trigger immediate MST page publication (for testing/debugging)
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id))]
    pub async fn trigger_sync(&self) -> StorageResult<()> {
        let mut mst_guard = self.mst.write().await;
        let _ = mst_guard.root_hash();

        // Extract page ranges in a way that doesn't borrow
        let ranges_result = mst_guard.serialise_page_ranges();
        let pages = {
            let pages_vec = match ranges_result {
                Some(ranges_slice) => ranges_slice.to_vec(),
                None => vec![],
            };
            pages_vec
        };

        if !pages.is_empty() {
            let network_pages = GenericNetworkPage::from_page_ranges(pages);

            self.networking
                .publish_pages(self.shard_id, network_pages)
                .await
                .map_err(|e| {
                    oprc_dp_storage::StorageError::serialization(&e.to_string())
                })?;
        }

        Ok(())
    }

    /// Signal readiness without starting networking (for testing only)
    #[cfg(test)]
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, collection = %self.metadata.collection, partition_id = %self.metadata.partition_id))]
    pub fn signal_readiness_for_test(&self) {
        let _ = self.readiness_sender.send(true);
    }
}

#[async_trait]
impl<
    S: StorageBackend + 'static,
    T: Clone
        + Send
        + Sync
        + std::hash::Hash
        + Serialize
        + for<'de> Deserialize<'de>
        + 'static,
> ReplicationLayer for MstReplicationLayer<S, T>
{
    fn replication_model(&self) -> ReplicationModel {
        ReplicationModel::ConflictFree {
            merge_strategy: crate::replication::MergeStrategy::LastWriterWins,
        }
    }

    async fn replicate_write(
        &self,
        request: crate::replication::ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        match request.operation {
            crate::replication::Operation::Write(write_op) => {
                let raw_key = write_op.key.as_slice();
                let value =
                    (self.config.deserialize)(write_op.value.as_slice())?;
                let old_value = if write_op.return_old {
                    self.set_with_return_old_raw(raw_key, value).await?
                } else {
                    self.set_raw(raw_key, value).await?;
                    None
                };
                Ok(ReplicationResponse {
                    status: crate::replication::ResponseStatus::Applied,
                    data: old_value,
                    ..Default::default()
                })
            }
            crate::replication::Operation::Delete(delete_op) => {
                let raw_key = delete_op.key.as_slice();
                self.delete_raw(raw_key).await?;
                Ok(ReplicationResponse {
                    status: crate::replication::ResponseStatus::Applied,
                    ..Default::default()
                })
            }
            _ => Err(ReplicationError::StorageError(
                oprc_dp_storage::StorageError::invalid_operation(
                    "Expected write or delete operation",
                ),
            )),
        }
    }

    async fn replicate_read(
        &self,
        request: crate::replication::ShardRequest,
    ) -> Result<ReplicationResponse, ReplicationError> {
        // Extract read operation
        if let crate::replication::Operation::Read(read_op) = request.operation
        {
            let raw_key = read_op.key.as_slice();
            if let Some(value) = self.get_raw(raw_key).await? {
                let serialized = (self.config.serialize)(&value)?;
                Ok(ReplicationResponse {
                    status: crate::replication::ResponseStatus::Applied,
                    data: Some(StorageValue::from(serialized)),
                    ..Default::default()
                })
            } else {
                Ok(ReplicationResponse {
                    status: crate::replication::ResponseStatus::Applied,
                    ..Default::default()
                })
            }
        } else {
            Err(ReplicationError::StorageError(
                oprc_dp_storage::StorageError::invalid_operation(
                    "Expected read operation",
                ),
            ))
        }
    }

    async fn add_replica(
        &self,
        _node_id: u64,
        _address: String,
    ) -> Result<(), ReplicationError> {
        // NoOps, MST not keeps the state of peer
        Ok(())
    }

    async fn remove_replica(
        &self,
        _node_id: u64,
    ) -> Result<(), ReplicationError> {
        // NoOps, MST not keeps the state of peer
        Ok(())
    }

    async fn get_replication_status(
        &self,
    ) -> Result<crate::replication::ReplicationStatus, ReplicationError> {
        Ok(crate::replication::ReplicationStatus {
            model: self.replication_model(),
            healthy_replicas: 1, // For MST, all connected Zenoh peers are healthy
            total_replicas: 1, // Would need Zenoh peer discovery to track this
            lag_ms: Some(0),   // MST is eventually consistent, lag is minimal
            conflicts: 0, // Conflicts are resolved automatically via merge function
            is_leader: false, // MST doesn't have leaders
            leader_id: None, // MST doesn't have leaders
            last_sync: Some(std::time::SystemTime::now()), // Last publication time
        })
    }

    async fn sync_replicas(&self) -> Result<(), ReplicationError> {
        // Trigger immediate synchronization
        self.trigger_sync().await?;
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }

    async fn initialize(&self) -> Result<(), ReplicationError> {
        // Call the MST-specific initialize method
        MstReplicationLayer::initialize(self).await?;
        Ok(())
    }
}
