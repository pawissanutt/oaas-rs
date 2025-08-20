use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    events::{EventConfig, EventManagerImpl, TriggerProcessor},
    replication::{
        mst::{MstConfig, MstReplicationLayer},
        no_replication::NoReplication,
        raft::OpenRaftReplicationLayer,
    },
    shard::unified::{BoxedUnifiedObjectShard, IntoUnifiedShard},
};
use oprc_dp_storage::{MemoryStorage, StorageError, StorageFactory};

use super::{
    config::ShardError, object_shard::ObjectUnifiedShard, traits::ShardMetadata,
};
use crate::shard::basic::ObjectEntry;

/// Factory for creating unified ObjectUnifiedShard instances with different storage and replication configurations
pub struct UnifiedShardFactory {
    session_pool: oprc_zenoh::pool::Pool,
    event_config: Option<EventConfig>,
}

impl UnifiedShardFactory {
    /// Create a new factory with session pool
    pub fn new(session_pool: oprc_zenoh::pool::Pool) -> Self {
        Self {
            session_pool,
            event_config: None,
        }
    }

    /// Create a new factory with session pool and event configuration
    pub fn new_with_events(
        session_pool: oprc_zenoh::pool::Pool,
        event_config: EventConfig,
    ) -> Self {
        Self {
            session_pool,
            event_config: Some(event_config),
        }
    }

    /// Create a unified shard with full networking (no replication)
    pub async fn create_basic_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<
        ObjectUnifiedShard<
            MemoryStorage,
            NoReplication<MemoryStorage>,
            EventManagerImpl<MemoryStorage>,
        >,
        ShardError,
    > {
        info!(
            "Creating networked no-replication unified shard: {:?}",
            &metadata
        );

        // Create storage backend
        let app_storage = StorageFactory::create_memory()
            .await
            .map_err(|e| ShardError::StorageError(e))?;

        // Create no-replication layer
        let replication = NoReplication::new(app_storage.clone());

        // Get Zenoh session for networking
        let z_session = self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })?;

        // Create event manager in factory if event config is available
        let event_manager = if let Some(config) = &self.event_config {
            let trigger_processor = Arc::new(TriggerProcessor::new(
                z_session.clone(),
                config.clone(),
            ));
            Some(Arc::new(EventManagerImpl::new(
                trigger_processor,
                app_storage.clone(),
            )))
        } else {
            None
        };

        // Create the unified shard with full networking
        ObjectUnifiedShard::new_full(
            metadata,
            app_storage,
            replication,
            z_session,
            event_manager,
        )
        .await
    }

    /// Create a unified shard with MST replication
    pub async fn create_mst_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<
        ObjectUnifiedShard<
            MemoryStorage,
            MstReplicationLayer<MemoryStorage, ObjectEntry>,
            EventManagerImpl<MemoryStorage>,
        >,
        ShardError,
    > {
        info!("Creating MST-replicated unified shard: {:?}", &metadata);

        let mst_config = MstConfig {
            extract_timestamp: Box::new(|entry: &ObjectEntry| {
                entry.last_updated
            }),
            merge_function: Box::new(|mut a, b, _| {
                a.merge(b).unwrap_or_else(|e| {
                    tracing::warn!("Merge failed: {}, using local value", e);
                });
                a
            }),
            serialize: Box::new(|entry| {
                bincode::serde::encode_to_vec(
                    entry,
                    bincode::config::standard(),
                )
                .map_err(|e| StorageError::serialization(&e.to_string()))
            }),
            deserialize: Box::new(|data| {
                bincode::serde::decode_from_slice(
                    data,
                    bincode::config::standard(),
                )
                .map(|(entry, _)| entry) // bincode v2 returns (T, bytes_read)
                .map_err(|e| StorageError::serialization(&e.to_string()))
            }),
        };

        // Create storage backend
        let app_storage = StorageFactory::create_memory()
            .await
            .map_err(|e| ShardError::StorageError(e))?;

        // Get Zenoh session for networking
        let z_session = self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })?;

        debug!("Creating replication layer");
        // Create MST replication layer with integrated networking
        let replication = MstReplicationLayer::new(
            app_storage.clone(),
            metadata.owner.unwrap_or_default(), // Use shard ID as node ID for now
            metadata.clone(),
            mst_config,
            z_session.clone(),
        );

        debug!("Creating event manager");
        // Create event manager in factory if event config is available
        let event_manager = if let Some(config) = &self.event_config {
            let trigger_processor = Arc::new(TriggerProcessor::new(
                z_session.clone(),
                config.clone(),
            ));
            Some(Arc::new(EventManagerImpl::new(
                trigger_processor,
                app_storage.clone(),
            )))
        } else {
            None
        };

        // Create the unified shard with MST replication
        ObjectUnifiedShard::new_full(
            metadata,
            app_storage,
            replication,
            z_session,
            event_manager,
        )
        .await
    }

    /// Create a unified shard with Raft replication
    pub async fn create_raft_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<
        ObjectUnifiedShard<
            MemoryStorage,
            OpenRaftReplicationLayer<MemoryStorage>,
            EventManagerImpl<MemoryStorage>,
        >,
        ShardError,
    > {
        info!("Creating Raft-replicated unified shard: {:?}", &metadata);

        // Create storage backend
        let app_storage = StorageFactory::create_memory()
            .await
            .map_err(|e| ShardError::StorageError(e))?;

        // Get Zenoh session for networking
        let z_session = self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })?;

        // Extract node ID from metadata (use shard ID if owner not specified)
        let node_id = metadata.owner.unwrap_or(metadata.id);

        // Create Raft replication layer
        let replication = OpenRaftReplicationLayer::new(
            node_id,
            app_storage.clone(),
            metadata.clone(),
            z_session.clone(),
            format!("oprc/{}/{}", metadata.collection, metadata.partition_id), // Use standard prefix pattern
        )
        .await?;

        // Create event manager in factory if event config is available
        let event_manager = if let Some(config) = &self.event_config {
            let trigger_processor = Arc::new(TriggerProcessor::new(
                z_session.clone(),
                config.clone(),
            ));
            Some(Arc::new(EventManagerImpl::new(
                trigger_processor,
                app_storage.clone(),
            )))
        } else {
            None
        };

        // Create the unified shard with Raft replication
        ObjectUnifiedShard::new_full(
            metadata,
            app_storage,
            replication,
            z_session,
            event_manager,
        )
        .await
    }

    /// Create a shard based on the metadata configuration
    pub async fn create_shard_from_metadata(
        &self,
        metadata: ShardMetadata,
    ) -> Result<BoxedUnifiedObjectShard, ShardError> {
        let shard_type = metadata.shard_type.to_lowercase();

        match shard_type.as_str() {
            "raft" => self
                .create_raft_shard(metadata)
                .await
                .map(|s| IntoUnifiedShard::into_boxed(s)),
            "mst" => self
                .create_mst_shard(metadata)
                .await
                .map(|s| IntoUnifiedShard::into_boxed(s)),
            "none" | "basic" | "single" => self
                .create_basic_shard(metadata)
                .await
                .map(|s| IntoUnifiedShard::into_boxed(s)),
            _ => {
                // Default to no-replication for unknown types
                info!(
                    "Unknown shard type '{}', defaulting to no-replication",
                    shard_type
                );
                self.create_basic_shard(metadata)
                    .await
                    .map(|s| IntoUnifiedShard::into_boxed(s))
            }
        }
    }
}

/// Type aliases for commonly used unified shard configurations
pub type NoReplicationUnifiedShard = ObjectUnifiedShard<
    MemoryStorage,
    NoReplication<MemoryStorage>,
    EventManagerImpl<MemoryStorage>,
>;

pub type RaftReplicationUnifiedShard = ObjectUnifiedShard<
    MemoryStorage,
    OpenRaftReplicationLayer<MemoryStorage>,
    EventManagerImpl<MemoryStorage>,
>;

pub type MstReplicationUnifiedShard = ObjectUnifiedShard<
    MemoryStorage,
    MstReplicationLayer<MemoryStorage, ObjectEntry>,
    EventManagerImpl<MemoryStorage>,
>;

/// Example patterns for creating different types of unified shards
pub mod patterns {
    use super::*;

    /// Pattern for creating a memory-only shard with no replication
    pub async fn create_memory_no_replication_pattern() -> Result<(), ShardError>
    {
        info!("Pattern: Memory storage + No replication");
        Ok(())
    }

    /// Pattern for creating a persistent storage shard with no replication  
    pub async fn create_persistent_no_replication_pattern()
    -> Result<(), ShardError> {
        info!("Pattern: Persistent storage + No replication");
        Ok(())
    }

    /// Pattern for creating a Raft-replicated shard
    pub async fn create_raft_replication_pattern() -> Result<(), ShardError> {
        info!("Pattern: Storage + Raft replication");
        Ok(())
    }

    /// Pattern for creating an MST-replicated shard
    pub async fn create_mst_replication_pattern() -> Result<(), ShardError> {
        info!("Pattern: Storage + MST replication");
        Ok(())
    }
}
