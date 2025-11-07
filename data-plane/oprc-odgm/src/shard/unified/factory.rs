use std::sync::Arc;
use tracing::{debug, info};

use crate::{
    events::{
        BridgeConfig, BridgeDispatcher, EventConfig, EventManagerImpl,
        TriggerProcessor, V2Dispatcher, V2DispatcherRef,
        bridge::run_bridge_consumer,
    },
    replication::{
        mst::{MstConfig, MstReplicationLayer},
        no_replication::NoReplication,
        raft::OpenRaftReplicationLayer,
    },
    shard::unified::{BoxedUnifiedObjectShard, IntoUnifiedShard},
};
use oprc_dp_storage::{AnyStorage, StorageConfig};

use super::{
    config::ShardError, object_shard::ObjectUnifiedShard, traits::ShardMetadata,
};
use oprc_dp_storage::StorageValue;

/// Tunable flags shared across unified shard construction paths.
#[derive(Clone, Copy, Debug)]
pub struct UnifiedShardConfig {
    pub max_string_id_len: usize,
    pub granular_prefetch_limit: usize,
}

/// Factory for creating unified ObjectUnifiedShard instances with different storage and replication configurations
pub struct UnifiedShardFactory {
    session_pool: oprc_zenoh::pool::Pool,
    event_config: Option<EventConfig>,
    config: UnifiedShardConfig,
    bridge: Option<crate::events::BridgeDispatcherRef>,
}

impl UnifiedShardFactory {
    /// Create a new factory with session pool
    pub fn new(
        session_pool: oprc_zenoh::pool::Pool,
        config: UnifiedShardConfig,
    ) -> Self {
        let bridge = Self::init_bridge();
        Self {
            session_pool,
            event_config: None,
            config,
            bridge,
        }
    }

    /// Create a new factory with session pool and event configuration
    pub fn new_with_events(
        session_pool: oprc_zenoh::pool::Pool,
        event_config: EventConfig,
        config: UnifiedShardConfig,
    ) -> Self {
        let bridge = Self::init_bridge();
        Self {
            session_pool,
            event_config: Some(event_config),
            config,
            bridge,
        }
    }

    /// Create a unified shard with full networking (no replication)
    pub async fn create_basic_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<
        ObjectUnifiedShard<
            AnyStorage,
            NoReplication<AnyStorage>,
            EventManagerImpl<AnyStorage>,
        >,
        ShardError,
    > {
        info!(
            "Creating networked no-replication unified shard: {:?}",
            &metadata
        );

        // Create storage backend
        let app_storage = self.build_app_storage()?;

        // Create no-replication layer
        let replication = NoReplication::new(app_storage.clone());

        // Get Zenoh session for networking
        let z_session = self.get_zenoh_session().await?;

        // Create event manager in factory if event config is available
        let event_manager = self.build_event_manager(&z_session, &app_storage);

        // Build V2 dispatcher (may allocate TriggerProcessor) before moving session
        let z_session_clone = z_session.clone();
        let v2 = self.build_v2_dispatcher(&z_session_clone);
        // Create the unified shard with full networking
        ObjectUnifiedShard::new_full(
            metadata,
            app_storage,
            replication,
            z_session,
            event_manager,
            self.config,
            self.bridge.clone(),
            v2,
        )
        .await
    }

    /// Create a unified shard with MST replication
    pub async fn create_mst_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<
        ObjectUnifiedShard<
            AnyStorage,
            MstReplicationLayer<AnyStorage, StorageValue>,
            EventManagerImpl<AnyStorage>,
        >,
        ShardError,
    > {
        info!("Creating MST-replicated unified shard: {:?}", &metadata);

        let mst_config = Self::build_mst_config();

        // Create storage backend
        let app_storage = self.build_app_storage()?;

        // Get Zenoh session for networking
        let z_session = self.get_zenoh_session().await?;

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
        let event_manager = self.build_event_manager(&z_session, &app_storage);

        let z_session_clone = z_session.clone();
        let v2 = self.build_v2_dispatcher(&z_session_clone);
        // Create the unified shard with MST replication
        ObjectUnifiedShard::new_full(
            metadata,
            app_storage,
            replication,
            z_session,
            event_manager,
            self.config,
            self.bridge.clone(),
            v2,
        )
        .await
    }

    /// Create a unified shard with Raft replication
    pub async fn create_raft_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<
        ObjectUnifiedShard<
            AnyStorage,
            OpenRaftReplicationLayer<AnyStorage>,
            EventManagerImpl<AnyStorage>,
        >,
        ShardError,
    > {
        info!("Creating Raft-replicated unified shard: {:?}", &metadata);

        // Create storage backend
        let app_storage = self.build_app_storage()?;

        // Get Zenoh session for networking
        let z_session = self.get_zenoh_session().await?;

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
        let event_manager = self.build_event_manager(&z_session, &app_storage);

        let z_session_clone = z_session.clone();
        let v2 = self.build_v2_dispatcher(&z_session_clone);
        // Create the unified shard with Raft replication
        ObjectUnifiedShard::new_full(
            metadata,
            app_storage,
            replication,
            z_session,
            event_manager,
            self.config,
            self.bridge.clone(),
            v2,
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

impl UnifiedShardFactory {
    fn build_app_storage(&self) -> Result<AnyStorage, ShardError> {
        StorageConfig::skiplist()
            .open_any()
            .map_err(|e| ShardError::StorageError(e))
    }

    async fn get_zenoh_session(&self) -> Result<zenoh::Session, ShardError> {
        self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })
    }

    fn build_event_manager(
        &self,
        z_session: &zenoh::Session,
        app_storage: &AnyStorage,
    ) -> Option<Arc<EventManagerImpl<AnyStorage>>> {
        self.event_config.as_ref().map(|config| {
            let trigger_processor = Arc::new(TriggerProcessor::new(
                z_session.clone(),
                config.clone(),
            ));
            Arc::new(EventManagerImpl::new(
                trigger_processor,
                app_storage.clone(),
            ))
        })
    }

    fn init_bridge() -> Option<crate::events::BridgeDispatcherRef> {
        let cfg = BridgeConfig::default();
        if !cfg.enabled {
            return None;
        }
        let (tx, rx) = tokio::sync::mpsc::channel(
            std::env::var("ODGM_EVENT_QUEUE_BOUND")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1024),
        );
        let (btx, _brx) = tokio::sync::broadcast::channel(
            std::env::var("ODGM_EVENT_BCAST_BOUND")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(256),
        );
        let dispatcher = Arc::new(BridgeDispatcher::new(tx, btx.clone()));
        // spawn consumer
        tokio::spawn(run_bridge_consumer(rx, cfg, btx));
        Some(dispatcher)
    }

    fn build_v2_dispatcher(
        &self,
        z_session: &zenoh::Session,
    ) -> Option<V2DispatcherRef> {
        // Default to enabling the V2 event pipeline unless explicitly disabled via env.
        // This activates per-entry fanout with proper Zenoh async invocation when events are enabled.
        let enabled = std::env::var("ODGM_EVENT_PIPELINE_V2")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true);
        if !enabled {
            return None;
        }
        let queue_bound = std::env::var("ODGM_EVENT_QUEUE_BOUND")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024);
        let bcast_bound = std::env::var("ODGM_EVENT_BCAST_BOUND")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(256);
        // Only build TriggerProcessor if we have event_config; otherwise we pass None (still logs changes).
        let tp = self.event_config.as_ref().map(|cfg| {
            Arc::new(TriggerProcessor::new(z_session.clone(), cfg.clone()))
        });
        Some(V2Dispatcher::new_with_processor(
            queue_bound,
            bcast_bound,
            tp,
        ))
    }

    fn build_mst_config() -> MstConfig<StorageValue> {
        // Opaque byte replication (granular values + metadata). We don't attempt
        // semantic merging yet; last-writer is represented by arrival order.
        MstConfig {
            extract_timestamp: Box::new(|_v: &StorageValue| 0),
            merge_function: Box::new(|_local, remote, _node_id| remote),
            serialize: Box::new(|v: &StorageValue| Ok(v.as_slice().to_vec())),
            deserialize: Box::new(|bytes: &[u8]| Ok(StorageValue::from(bytes))),
        }
    }
}

/// Type aliases for commonly used unified shard configurations
pub type NoReplicationUnifiedShard = ObjectUnifiedShard<
    AnyStorage,
    NoReplication<AnyStorage>,
    EventManagerImpl<AnyStorage>,
>;

pub type RaftReplicationUnifiedShard = ObjectUnifiedShard<
    AnyStorage,
    OpenRaftReplicationLayer<AnyStorage>,
    EventManagerImpl<AnyStorage>,
>;

pub type MstReplicationUnifiedShard = ObjectUnifiedShard<
    AnyStorage,
    MstReplicationLayer<AnyStorage, StorageValue>,
    EventManagerImpl<AnyStorage>,
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
