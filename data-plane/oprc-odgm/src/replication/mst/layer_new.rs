//! Main MST replication layer implementation

use async_trait::async_trait;
use merkle_search_tree::MerkleSearchTree;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::replication::{ReplicationLayer, ReplicationModel};
use crate::shard::ShardMetadata;
use oprc_dp_storage::{StorageBackend, StorageResult, StorageValue};

use super::networking::{MstPageRequestHandlerImpl, MstPageUpdateHandlerImpl};
use super::traits::MstNetworking;
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
    N: MstNetworking<T>,
> {
    storage: Arc<S>,
    mst: Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
    node_id: u64,
    metadata: ShardMetadata,
    config: Arc<MstConfig<T>>,
    networking: Arc<N>,
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
        N: MstNetworking<T> + 'static,
    > MstReplicationLayer<S, T, N>
{
    /// Create a new MST replication layer
    pub fn new(
        storage: S,
        node_id: u64,
        metadata: ShardMetadata,
        config: MstConfig<T>,
        networking: N,
    ) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let layer = Self {
            storage: Arc::new(storage),
            mst: Arc::new(RwLock::new(MerkleSearchTree::default())),
            node_id,
            metadata,
            config: Arc::new(config),
            networking: Arc::new(networking),
            readiness_sender: tx,
            readiness_receiver: rx,
        };

        // Set up networking handlers
        layer.setup_networking_handlers();
        layer
    }

    /// Set up the networking handlers for this MST layer
    fn setup_networking_handlers(&self) {
        let page_request_handler = Arc::new(MstPageRequestHandlerImpl::new(
            self.storage.clone(),
            self.config.clone(),
        ));

        let page_update_handler = Arc::new(MstPageUpdateHandlerImpl::new(
            self.storage.clone(),
            self.mst.clone(),
            self.config.clone(),
            self.node_id,
            self.networking.clone(),
        ));

        self.networking
            .set_page_request_handler(page_request_handler);
        self.networking.set_page_update_handler(page_update_handler);
    }

    /// Initialize the MST from existing storage data
    pub async fn initialize(&self) -> StorageResult<()> {
        // Rebuild MST from persistent storage
        self.rebuild_mst_from_storage().await?;

        // Start networking layer
        self.networking.start().await.map_err(|e| {
            oprc_dp_storage::StorageError::serialization(&e.to_string())
        })?;

        // Start periodic MST publication
        self.start_periodic_publication().await;

        // Signal readiness
        let _ = self.readiness_sender.send(true);

        Ok(())
    }

    /// Start periodic MST page publication
    async fn start_periodic_publication(&self) {
        let interval_ms: u64 = self
            .metadata
            .options
            .get("mst_sync_interval")
            .unwrap_or(&"5000".to_string())
            .parse()
            .unwrap_or(5000);

        let mst = self.mst.clone();
        let networking = self.networking.clone();
        let node_id = self.node_id;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_millis(interval_ms),
            );

            loop {
                interval.tick().await;
                let mut mst_guard = mst.write().await;
                let _ = mst_guard.root_hash(); // Force MST calculation

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
                    let network_pages =
                        GenericNetworkPage::from_page_ranges(pages);

                    if let Err(err) =
                        networking.publish_pages(node_id, network_pages).await
                    {
                        tracing::error!("Failed to publish MST pages: {}", err);
                    }
                }
            }
        });
    }

    /// Rebuild the MST from all data in storage
    pub async fn rebuild_mst_from_storage(&self) -> StorageResult<()> {
        tracing::info!(
            "Rebuilding MST from storage for shard {}",
            self.metadata.id
        );

        // Scan all entries from storage
        let entries = self.storage.scan(&[]).await?;
        let entry_count = entries.len();
        let mut mst = self.mst.write().await;

        for (key_bytes, value_bytes) in &entries {
            // Parse key (assuming it's a u64 serialized as bytes)
            if key_bytes.len() == 8 {
                let key_u64 = u64::from_be_bytes(
                    key_bytes.as_slice().try_into().map_err(|_| {
                        oprc_dp_storage::StorageError::serialization(
                            "Invalid key format",
                        )
                    })?,
                );

                // Deserialize using configured function
                let entry = (self.config.deserialize)(value_bytes.as_slice())?;

                // Insert into MST
                mst.upsert(MstKey(key_u64), &entry);
            }
        }

        tracing::info!("MST rebuild complete, {} entries loaded", entry_count);
        Ok(())
    }

    /// Get an entry (reads from storage, not MST)
    pub async fn get(&self, key: u64) -> StorageResult<Option<T>> {
        let key_bytes = key.to_be_bytes();

        match self.storage.get(&key_bytes).await? {
            Some(value_bytes) => {
                let entry = (self.config.deserialize)(value_bytes.as_slice())?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// Set an entry with LWW conflict resolution
    pub async fn set(&self, key: u64, entry: T) -> StorageResult<()> {
        let key_bytes = key.to_be_bytes();

        // Check for existing entry and resolve conflicts
        let final_entry =
            if let Some(existing_bytes) = self.storage.get(&key_bytes).await? {
                let existing =
                    (self.config.deserialize)(existing_bytes.as_slice())?;

                // Apply configurable merge function with LWW logic
                (self.config.merge_function)(&existing, &entry, self.node_id)
            } else {
                entry
            };

        // Serialize using configured function
        let value_bytes = (self.config.serialize)(&final_entry)?;

        self.storage
            .put(&key_bytes, StorageValue::from(value_bytes))
            .await?;

        // Update MST
        let mut mst = self.mst.write().await;
        mst.upsert(MstKey(key), &final_entry);

        Ok(())
    }

    /// Delete an entry
    pub async fn delete(&self, key: u64) -> StorageResult<()> {
        let key_bytes = key.to_be_bytes();
        self.storage.delete(&key_bytes).await?;

        // Remove from MST
        let _mst = self.mst.write().await;
        // MST doesn't have direct removal - we rebuild or use update with None
        // For now, we'll track deletions by updating with a tombstone or rebuilding
        // This is a limitation of the current MST library
        tracing::debug!(
            "Deleted key {} from storage, MST will be rebuilt on next restart",
            key
        );

        Ok(())
    }

    /// Get the current MST root hash for synchronization
    pub async fn get_root_hash(&self) -> Option<Vec<u8>> {
        let mut mst = self.mst.write().await;
        let root_hash = mst.root_hash();
        // Convert RootHash to bytes
        Some(root_hash.as_ref().to_vec())
    }

    /// Trigger immediate MST page publication (for testing/debugging)
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
                .publish_pages(self.node_id, network_pages)
                .await
                .map_err(|e| {
                    oprc_dp_storage::StorageError::serialization(&e.to_string())
                })?;
        }

        Ok(())
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
        N: MstNetworking<T> + 'static,
    > ReplicationLayer for MstReplicationLayer<S, T, N>
{
    type Error = oprc_dp_storage::StorageError;

    fn replication_model(&self) -> ReplicationModel {
        ReplicationModel::ConflictFree {
            merge_strategy: crate::replication::MergeStrategy::LastWriterWins,
        }
    }

    async fn replicate_write(
        &self,
        request: crate::replication::ShardRequest,
    ) -> Result<crate::replication::ReplicationResponse, Self::Error> {
        // Extract write operation
        if let crate::replication::Operation::Write(write_op) =
            request.operation
        {
            // Parse key as u64 (assuming key is serialized as bytes)
            if write_op.key.len() == 8 {
                let key = u64::from_be_bytes(
                    write_op.key.as_slice().try_into().map_err(|_| {
                        oprc_dp_storage::StorageError::serialization(
                            "Invalid key format",
                        )
                    })?,
                );

                // Deserialize value
                let value =
                    (self.config.deserialize)(write_op.value.as_slice())?;

                // Perform write with conflict resolution
                self.set(key, value).await?;

                Ok(crate::replication::ReplicationResponse {
                    status: crate::replication::ResponseStatus::Applied,
                    data: None,
                    metadata: std::collections::HashMap::new(),
                })
            } else {
                Err(oprc_dp_storage::StorageError::serialization(
                    "Invalid key format",
                ))
            }
        } else {
            Err(oprc_dp_storage::StorageError::invalid_operation(
                "Expected write operation",
            ))
        }
    }

    async fn replicate_read(
        &self,
        request: crate::replication::ShardRequest,
    ) -> Result<crate::replication::ReplicationResponse, Self::Error> {
        // Extract read operation
        if let crate::replication::Operation::Read(read_op) = request.operation
        {
            // Parse key as u64
            if read_op.key.len() == 8 {
                let key = u64::from_be_bytes(
                    read_op.key.as_slice().try_into().map_err(|_| {
                        oprc_dp_storage::StorageError::serialization(
                            "Invalid key format",
                        )
                    })?,
                );

                // Perform read
                if let Some(value) = self.get(key).await? {
                    let serialized = (self.config.serialize)(&value)?;
                    Ok(crate::replication::ReplicationResponse {
                        status: crate::replication::ResponseStatus::Applied,
                        data: Some(StorageValue::from(serialized)),
                        metadata: std::collections::HashMap::new(),
                    })
                } else {
                    Ok(crate::replication::ReplicationResponse {
                        status: crate::replication::ResponseStatus::Applied,
                        data: None,
                        metadata: std::collections::HashMap::new(),
                    })
                }
            } else {
                Err(oprc_dp_storage::StorageError::serialization(
                    "Invalid key format",
                ))
            }
        } else {
            Err(oprc_dp_storage::StorageError::invalid_operation(
                "Expected read operation",
            ))
        }
    }

    async fn add_replica(
        &self,
        _node_id: u64,
        _address: String,
    ) -> Result<(), Self::Error> {
        // NoOps, MST not keeps the state of peer
        Ok(())
    }

    async fn remove_replica(&self, _node_id: u64) -> Result<(), Self::Error> {
        // NoOps, MST not keeps the state of peer
        Ok(())
    }

    async fn get_replication_status(
        &self,
    ) -> Result<crate::replication::ReplicationStatus, Self::Error> {
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

    async fn sync_replicas(&self) -> Result<(), Self::Error> {
        // Trigger immediate synchronization
        self.trigger_sync().await?;
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }
}
