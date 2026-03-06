//! Idiomatic Rust alternative to the Factory pattern for shard creation.
//!
//! Uses a builder pattern with type-state to ensure correct configuration
//! at compile time, plus associated constructors for common cases.

use std::sync::Arc;
use tracing::info;

use crate::{
    events::{
        EventConfig, EventManagerImpl, TriggerProcessor, V2Dispatcher,
        V2DispatcherRef,
    },
    replication::{
        ReplicationLayer,
        mst::{MstConfig, MstReplicationLayer},
        no_replication::NoReplication,
        raft::OpenRaftReplicationLayer,
    },
};
use oprc_dp_storage::{
    AnyStorage, ApplicationDataStorage, StorageConfig, StorageValue,
};
use oprc_zenoh::pool::Pool;

use super::object_shard::ObjectUnifiedShard;
use super::object_trait::{BoxedUnifiedObjectShard, IntoUnifiedShard};
use super::{ShardError, ShardMetadata};

// ============================================================================
// Configuration types
// ============================================================================

/// Tunable flags for unified shard construction.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShardOptions {
    pub max_string_id_len: usize,
    pub granular_prefetch_limit: usize,
}

impl ShardOptions {
    pub fn new(
        max_string_id_len: usize,
        granular_prefetch_limit: usize,
    ) -> Self {
        Self {
            max_string_id_len,
            granular_prefetch_limit,
        }
    }
}

/// Event pipeline configuration, extracted from environment by default.
#[derive(Clone, Debug)]
pub struct EventPipelineConfig {
    pub enabled: bool,
    pub queue_bound: usize,
    pub broadcast_bound: usize,
    /// When true, MST sync writes emit `MutationSource::Sync` events.
    /// Disabled by default to avoid overhead for deployments that do not consume sync events.
    pub mst_sync_events: bool,
    /// When true, processed events are published to Zenoh
    /// for consumption by Gateway WebSocket handlers.
    pub zenoh_event_publish: bool,
    /// Zenoh locality for event publishing.
    /// Controls whether events stay in-process (`SessionLocal`), go to remote
    /// peers only (`Remote`), or both (`Any`). Default: `Remote`.
    pub zenoh_event_locality: zenoh::sample::Locality,
}

impl Default for EventPipelineConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl EventPipelineConfig {
    pub fn from_env() -> Self {
        let enabled = std::env::var("ODGM_EVENT_PIPELINE_V2")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true);
        let queue_bound = std::env::var("ODGM_EVENT_QUEUE_BOUND")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024);
        let broadcast_bound = std::env::var("ODGM_EVENT_BCAST_BOUND")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(256);
        let mst_sync_events = std::env::var("ODGM_MST_SYNC_EVENTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);
        let zenoh_event_publish = std::env::var("ODGM_ZENOH_EVENT_PUBLISH")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);
        let zenoh_event_locality = match std::env::var("ODGM_ZENOH_EVENT_LOCALITY")
            .ok()
            .as_deref()
        {
            Some("session_local") => zenoh::sample::Locality::SessionLocal,
            Some("any") => zenoh::sample::Locality::Any,
            _ => zenoh::sample::Locality::Remote,
        };
        Self {
            enabled,
            queue_bound,
            broadcast_bound,
            mst_sync_events,
            zenoh_event_publish,
            zenoh_event_locality,
        }
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            queue_bound: 0,
            broadcast_bound: 0,
            mst_sync_events: false,
            zenoh_event_publish: false,
            zenoh_event_locality: zenoh::sample::Locality::Remote,
        }
    }
}

// ============================================================================
// Builder pattern with type-state
// ============================================================================

/// Builder for constructing `ObjectUnifiedShard` instances.
///
/// Uses type-state pattern: you must provide metadata and storage before building.
pub struct ShardBuilder<S = (), R = ()> {
    metadata: Option<ShardMetadata>,
    storage: S,
    replication: R,
    session: Option<zenoh::Session>,
    event_config: Option<EventConfig>,
    options: ShardOptions,
    event_pipeline_config: EventPipelineConfig,
}

impl Default for ShardBuilder<(), ()> {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardBuilder<(), ()> {
    /// Create a new builder with default options.
    pub fn new() -> Self {
        Self {
            metadata: None,
            storage: (),
            replication: (),
            session: None,
            event_config: None,
            options: ShardOptions::default(),
            event_pipeline_config: EventPipelineConfig::default(),
        }
    }
}

impl<S, R> ShardBuilder<S, R> {
    /// Set the shard metadata (required).
    pub fn metadata(mut self, metadata: ShardMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set the Zenoh session for networking.
    pub fn session(mut self, session: zenoh::Session) -> Self {
        self.session = Some(session);
        self
    }

    /// Set the event configuration for triggers.
    pub fn events(mut self, config: EventConfig) -> Self {
        self.event_config = Some(config);
        self
    }

    /// Set shard tuning options.
    pub fn options(mut self, options: ShardOptions) -> Self {
        self.options = options;
        self
    }

    /// Set V2 event pipeline configuration.
    pub fn event_pipeline_config(
        mut self,
        config: EventPipelineConfig,
    ) -> Self {
        self.event_pipeline_config = config;
        self
    }
}

// Type-state transitions for storage
impl<R> ShardBuilder<(), R> {
    /// Set the storage backend (type-state transition).
    pub fn storage<S: ApplicationDataStorage>(
        self,
        storage: S,
    ) -> ShardBuilder<S, R> {
        ShardBuilder {
            metadata: self.metadata,
            storage,
            replication: self.replication,
            session: self.session,
            event_config: self.event_config,
            options: self.options,
            event_pipeline_config: self.event_pipeline_config,
        }
    }

    /// Use default in-memory storage.
    pub fn memory_storage(
        self,
    ) -> Result<ShardBuilder<AnyStorage, R>, ShardError> {
        let storage = StorageConfig::skiplist()
            .open_any()
            .map_err(ShardError::StorageError)?;
        Ok(ShardBuilder {
            metadata: self.metadata,
            storage,
            replication: self.replication,
            session: self.session,
            event_config: self.event_config,
            options: self.options,
            event_pipeline_config: self.event_pipeline_config,
        })
    }
}

// Type-state transitions for replication
impl<S: ApplicationDataStorage + Clone> ShardBuilder<S, ()> {
    /// Use no replication (single-node mode).
    pub fn no_replication(self) -> ShardBuilder<S, NoReplication<S>> {
        let replication = NoReplication::new(self.storage.clone());
        ShardBuilder {
            metadata: self.metadata,
            storage: self.storage,
            replication,
            session: self.session,
            event_config: self.event_config,
            options: self.options,
            event_pipeline_config: self.event_pipeline_config,
        }
    }
}

impl ShardBuilder<AnyStorage, ()> {
    /// Use MST replication (requires metadata and session).
    pub fn mst_replication(
        self,
    ) -> Result<
        ShardBuilder<AnyStorage, MstReplicationLayer<AnyStorage, StorageValue>>,
        ShardError,
    > {
        let metadata = self.metadata.as_ref().ok_or_else(|| {
            ShardError::ConfigurationError(
                "Metadata required for MST replication".into(),
            )
        })?;
        let session = self.session.as_ref().ok_or_else(|| {
            ShardError::ConfigurationError(
                "Session required for MST replication".into(),
            )
        })?;

        let mst_config = default_mst_config();

        // If MST sync events are enabled, build the V2 dispatcher now so the
        // MST networking layer can emit MutationSource::Sync events.
        let v2_for_mst = if self.event_pipeline_config.enabled
            && self.event_pipeline_config.mst_sync_events
        {
            Some(build_event_dispatcher(
                session,
                &self.event_config,
                &self.event_pipeline_config,
            ))
        } else {
            None
        }
        .flatten();

        let replication = MstReplicationLayer::new_with_dispatcher(
            self.storage.clone(),
            metadata.owner.unwrap_or_default(),
            metadata.clone(),
            mst_config,
            session.clone(),
            v2_for_mst,
        );

        Ok(ShardBuilder {
            metadata: self.metadata,
            storage: self.storage,
            replication,
            session: self.session,
            event_config: self.event_config,
            options: self.options,
            event_pipeline_config: self.event_pipeline_config,
        })
    }

    /// Use Raft replication (requires metadata and session).
    pub async fn raft_replication(
        self,
    ) -> Result<
        ShardBuilder<AnyStorage, OpenRaftReplicationLayer<AnyStorage>>,
        ShardError,
    > {
        let metadata = self.metadata.as_ref().ok_or_else(|| {
            ShardError::ConfigurationError(
                "Metadata required for Raft replication".into(),
            )
        })?;
        let session = self.session.as_ref().ok_or_else(|| {
            ShardError::ConfigurationError(
                "Session required for Raft replication".into(),
            )
        })?;

        let node_id = metadata.owner.unwrap_or(metadata.id);
        let prefix =
            format!("oprc/{}/{}", metadata.collection, metadata.partition_id);

        let replication = OpenRaftReplicationLayer::new(
            node_id,
            self.storage.clone(),
            metadata.clone(),
            session.clone(),
            prefix,
        )
        .await?;

        Ok(ShardBuilder {
            metadata: self.metadata,
            storage: self.storage,
            replication,
            session: self.session,
            event_config: self.event_config,
            options: self.options,
            event_pipeline_config: self.event_pipeline_config,
        })
    }
}

// Build methods (only available when storage and replication are configured)
impl<S, R> ShardBuilder<S, R>
where
    S: ApplicationDataStorage + Clone + Send + Sync + 'static,
    R: ReplicationLayer + 'static,
{
    /// Build a minimal shard without networking (for testing).
    pub async fn build_minimal(
        self,
    ) -> Result<ObjectUnifiedShard<S, R, EventManagerImpl<S>>, ShardError> {
        let metadata = self.metadata.ok_or_else(|| {
            ShardError::ConfigurationError("Metadata is required".into())
        })?;

        info!("Building minimal shard: {:?}", &metadata);

        ObjectUnifiedShard::new_minimal(
            metadata,
            self.storage,
            self.replication,
            self.options.into(),
        )
        .await
    }

    /// Build a full shard with networking.
    pub async fn build(
        self,
    ) -> Result<ObjectUnifiedShard<S, R, EventManagerImpl<S>>, ShardError> {
        let metadata = self.metadata.ok_or_else(|| {
            ShardError::ConfigurationError("Metadata is required".into())
        })?;
        let session = self.session.ok_or_else(|| {
            ShardError::ConfigurationError(
                "Session is required for full shard".into(),
            )
        })?;

        info!("Building networked shard: {:?}", &metadata);

        let event_manager = self.event_config.as_ref().map(|config| {
            let trigger_processor = Arc::new(TriggerProcessor::new(
                session.clone(),
                config.clone(),
            ));
            Arc::new(EventManagerImpl::new(
                trigger_processor,
                self.storage.clone(),
            ))
        });

        let v2_dispatcher = build_event_dispatcher(
            &session,
            &self.event_config,
            &self.event_pipeline_config,
        );

        ObjectUnifiedShard::new_full(
            metadata,
            self.storage,
            self.replication,
            session,
            event_manager,
            self.options.into(),
            v2_dispatcher,
        )
        .await
    }
}

// ============================================================================
// Convenience constructors (associated functions on ObjectUnifiedShard)
// ============================================================================

/// Extension trait providing convenient constructors for common shard configurations.
pub trait ShardConstructors: Sized {
    /// Create a basic in-memory shard with no replication (for testing).
    fn basic_memory(
        metadata: ShardMetadata,
        options: ShardOptions,
    ) -> impl std::future::Future<Output = Result<Self, ShardError>> + Send;

    /// Create a networked shard with no replication.
    fn networked(
        metadata: ShardMetadata,
        session: zenoh::Session,
        event_config: Option<EventConfig>,
        options: ShardOptions,
    ) -> impl std::future::Future<Output = Result<Self, ShardError>> + Send;
}

impl ShardConstructors
    for ObjectUnifiedShard<
        AnyStorage,
        NoReplication<AnyStorage>,
        EventManagerImpl<AnyStorage>,
    >
{
    async fn basic_memory(
        metadata: ShardMetadata,
        options: ShardOptions,
    ) -> Result<Self, ShardError> {
        ShardBuilder::new()
            .metadata(metadata)
            .options(options)
            .memory_storage()?
            .no_replication()
            .build_minimal()
            .await
    }

    async fn networked(
        metadata: ShardMetadata,
        session: zenoh::Session,
        event_config: Option<EventConfig>,
        options: ShardOptions,
    ) -> Result<Self, ShardError> {
        let mut builder = ShardBuilder::new()
            .metadata(metadata)
            .session(session)
            .options(options)
            .memory_storage()?
            .no_replication();

        if let Some(config) = event_config {
            builder = builder.events(config);
        }

        builder.build().await
    }
}

// ============================================================================
// Dynamic dispatch helper (for runtime shard type selection)
// ============================================================================

/// Create a boxed shard based on metadata's `shard_type` field.
///
/// This is the only place where we need dynamic dispatch - when the shard type
/// is determined at runtime (e.g., from configuration).
pub async fn create_shard_dynamic(
    metadata: ShardMetadata,
    session: zenoh::Session,
    event_config: Option<EventConfig>,
    options: ShardOptions,
) -> Result<BoxedUnifiedObjectShard, ShardError> {
    let shard_type = metadata.shard_type.to_lowercase();

    match shard_type.as_str() {
        "raft" => {
            info!("Creating Raft-replicated shard");
            let mut builder = ShardBuilder::new()
                .metadata(metadata)
                .session(session)
                .options(options)
                .memory_storage()?
                .raft_replication()
                .await?;

            if let Some(config) = event_config {
                builder = builder.events(config);
            }

            builder.build().await.map(IntoUnifiedShard::into_boxed)
        }
        "mst" => {
            info!("Creating MST-replicated shard");
            let mut builder = ShardBuilder::new()
                .metadata(metadata)
                .session(session)
                .options(options)
                .memory_storage()?
                .mst_replication()?;

            if let Some(config) = event_config {
                builder = builder.events(config);
            }

            builder.build().await.map(IntoUnifiedShard::into_boxed)
        }
        "none" | "basic" | "single" | _ => {
            if !matches!(shard_type.as_str(), "none" | "basic" | "single") {
                info!(
                    "Unknown shard type '{}', defaulting to no-replication",
                    shard_type
                );
            }

            let mut builder = ShardBuilder::new()
                .metadata(metadata)
                .session(session)
                .options(options)
                .memory_storage()?
                .no_replication();

            if let Some(config) = event_config {
                builder = builder.events(config);
            }

            builder.build().await.map(IntoUnifiedShard::into_boxed)
        }
    }
}

/// Convenience function to get a session and create a dynamic shard.
pub async fn create_shard_with_pool(
    metadata: ShardMetadata,
    pool: &Pool,
    event_config: Option<EventConfig>,
    options: ShardOptions,
) -> Result<BoxedUnifiedObjectShard, ShardError> {
    let session = pool.get_session().await.map_err(|e| {
        ShardError::ConfigurationError(format!(
            "Failed to get Zenoh session: {}",
            e
        ))
    })?;

    create_shard_dynamic(metadata, session, event_config, options).await
}

// ============================================================================
// Helper functions
// ============================================================================

fn build_event_dispatcher(
    session: &zenoh::Session,
    event_config: &Option<EventConfig>,
    event_pipeline_config: &EventPipelineConfig,
) -> Option<V2DispatcherRef> {
    if !event_pipeline_config.enabled {
        return None;
    }

    let trigger_processor = event_config.as_ref().map(|cfg| {
        Arc::new(TriggerProcessor::new(session.clone(), cfg.clone()))
    });

    let zenoh_session = if event_pipeline_config.zenoh_event_publish {
        Some((session.clone(), event_pipeline_config.zenoh_event_locality))
    } else {
        None
    };

    Some(V2Dispatcher::new_with_zenoh(
        event_pipeline_config.queue_bound,
        event_pipeline_config.broadcast_bound,
        trigger_processor,
        zenoh_session,
    ))
}

fn default_mst_config() -> MstConfig<StorageValue> {
    MstConfig {
        extract_timestamp: Box::new(|_v: &StorageValue| 0),
        merge_function: Box::new(|_local, remote, _node_id| remote),
        serialize: Box::new(|v: &StorageValue| Ok(v.as_slice().to_vec())),
        deserialize: Box::new(|bytes: &[u8]| Ok(StorageValue::from(bytes))),
    }
}

// ============================================================================
// Type aliases (same as before, for compatibility)
// ============================================================================

pub type BasicShard = ObjectUnifiedShard<
    AnyStorage,
    NoReplication<AnyStorage>,
    EventManagerImpl<AnyStorage>,
>;

pub type RaftShard = ObjectUnifiedShard<
    AnyStorage,
    OpenRaftReplicationLayer<AnyStorage>,
    EventManagerImpl<AnyStorage>,
>;

pub type MstShard = ObjectUnifiedShard<
    AnyStorage,
    MstReplicationLayer<AnyStorage, StorageValue>,
    EventManagerImpl<AnyStorage>,
>;

// ============================================================================
// Backward compatibility: UnifiedShardFactory (deprecated)
// ============================================================================

/// Backward-compatible factory for creating unified shards.
///
/// **Deprecated**: Use [`ShardBuilder`] or [`create_shard_with_pool`] instead.
#[deprecated(
    since = "0.1.0",
    note = "Use ShardBuilder or create_shard_with_pool instead"
)]
pub struct UnifiedShardFactory {
    session_pool: Pool,
    event_config: Option<EventConfig>,
    config: ShardOptions,
}

#[allow(deprecated)]
impl UnifiedShardFactory {
    /// Create a new factory with session pool
    pub fn new(session_pool: Pool, config: ShardOptions) -> Self {
        Self {
            session_pool,
            event_config: None,
            config,
        }
    }

    /// Create a new factory with session pool and event configuration
    pub fn new_with_events(
        session_pool: Pool,
        event_config: EventConfig,
        config: ShardOptions,
    ) -> Self {
        Self {
            session_pool,
            event_config: Some(event_config),
            config,
        }
    }

    /// Create a shard based on the metadata configuration
    pub async fn create_shard_from_metadata(
        &self,
        metadata: ShardMetadata,
    ) -> Result<BoxedUnifiedObjectShard, ShardError> {
        create_shard_with_pool(
            metadata,
            &self.session_pool,
            self.event_config.clone(),
            self.config,
        )
        .await
    }

    /// Create a unified shard with no replication (backward compat)
    pub async fn create_basic_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<BasicShard, ShardError> {
        let session = self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })?;

        let mut builder = ShardBuilder::new()
            .metadata(metadata)
            .session(session)
            .options(self.config)
            .memory_storage()?
            .no_replication();

        if let Some(config) = &self.event_config {
            builder = builder.events(config.clone());
        }

        builder.build().await
    }

    /// Create a unified shard with MST replication (backward compat)
    pub async fn create_mst_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<MstShard, ShardError> {
        let session = self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })?;

        let mut builder = ShardBuilder::new()
            .metadata(metadata)
            .session(session)
            .options(self.config)
            .memory_storage()?
            .mst_replication()?;

        if let Some(config) = &self.event_config {
            builder = builder.events(config.clone());
        }

        builder.build().await
    }

    /// Create a unified shard with Raft replication (backward compat)
    pub async fn create_raft_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<RaftShard, ShardError> {
        let session = self.session_pool.get_session().await.map_err(|e| {
            ShardError::ConfigurationError(format!(
                "Failed to get Zenoh session: {}",
                e
            ))
        })?;

        let mut builder = ShardBuilder::new()
            .metadata(metadata)
            .session(session)
            .options(self.config)
            .memory_storage()?
            .raft_replication()
            .await?;

        if let Some(config) = &self.event_config {
            builder = builder.events(config.clone());
        }

        builder.build().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_metadata() -> ShardMetadata {
        ShardMetadata {
            id: 1,
            collection: "test".into(),
            partition_id: 0,
            owner: None,
            primary: None,
            replica: vec![],
            replica_owner: vec![],
            shard_type: "basic".into(),
            options: Default::default(),
            invocations: Default::default(),
            storage_config: None,
            replication_config: None,
            consistency_config: None,
        }
    }

    #[tokio::test]
    async fn test_builder_minimal() {
        let shard = ShardBuilder::new()
            .metadata(test_metadata())
            .options(ShardOptions::new(160, 256))
            .memory_storage()
            .unwrap()
            .no_replication()
            .build_minimal()
            .await
            .unwrap();

        assert_eq!(shard.class_id(), "test");
    }

    #[tokio::test]
    async fn test_convenience_constructor() {
        let shard = BasicShard::basic_memory(
            test_metadata(),
            ShardOptions::new(160, 256),
        )
        .await
        .unwrap();

        assert_eq!(shard.class_id(), "test");
    }
}
