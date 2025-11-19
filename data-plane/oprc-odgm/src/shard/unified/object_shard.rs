use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

use super::config::{ShardError, ShardMetrics};
use super::factory::UnifiedShardConfig;
use super::network::UnifiedShardNetwork;
use super::object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard,
    ObjectShard, UnifiedShardTransaction,
};
use super::traits::ShardMetadata;
use crate::error::OdgmError;
use crate::events::{ChangedKey, MutAction, MutationContext};
use crate::events::{EventContext, EventManager};
use crate::granular_key::ObjectMetadata;
use crate::granular_trait::{EntryListOptions, EntryListResult, EntryStore};
use crate::replication::ReplicationLayer;
use crate::shard::{
    ObjectData, ObjectVal,
    invocation::{InvocationNetworkManager, InvocationOffloader},
    liveliness::MemberLivelinessState,
};
use crate::storage_key::string_object_event_config_key;
use oprc_dp_storage::{ApplicationDataStorage, StorageValue};
use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
};
use oprc_invoke::OffloadError;
use prost::Message as _;

/// Unified ObjectShard that combines storage, networking, events, and management
pub struct ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage,
    R: ReplicationLayer,
    E: EventManager + Send + Sync + 'static,
{
    // Core storage and metadata (merged from UnifiedShard)
    metadata: ShardMetadata,
    pub(crate) app_storage: A, // Direct access to application storage - log storage is now part of replication
    pub(crate) replication: Arc<R>, // Mandatory replication layer (use NoReplication for single-node)
    pub(crate) metrics: Arc<ShardMetrics>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,

    // Networking components (optional - can be disabled for storage-only use)
    z_session: Option<zenoh::Session>,
    pub(crate) inv_net_manager: Option<Arc<Mutex<InvocationNetworkManager<E>>>>,
    pub(crate) inv_offloader: Option<Arc<InvocationOffloader<E>>>,
    pub(crate) network: OnceCell<Arc<Mutex<UnifiedShardNetwork<R>>>>,

    // Event management (optional)
    event_manager: Option<Arc<E>>,

    // Liveliness management (optional)
    #[allow(dead_code)]
    liveliness_state: Option<MemberLivelinessState>,

    // Control and metadata
    token: CancellationToken,

    // Unified shard configuration
    config: UnifiedShardConfig,

    // V2 per-entry dispatcher (J2 skeleton)
    pub(crate) v2_dispatcher: Option<crate::events::V2DispatcherRef>,
}

impl<A, R, E> ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
    E: EventManager + Send + Sync + 'static,
{
    pub fn class_id(&self) -> &str {
        &self.metadata.collection
    }
    pub fn partition_id_u16(&self) -> u16 {
        self.metadata.partition_id as u16
    }
    pub fn v2_subscribe(
        &self,
    ) -> Option<
        tokio::sync::broadcast::Receiver<crate::events::v2::V2QueuedEvent>,
    > {
        self.v2_dispatcher.as_ref().map(|d| d.subscribe())
    }
    pub fn v2_emitted_events(&self) -> Option<u64> {
        self.v2_dispatcher
            .as_ref()
            .map(|d| d.metrics_emitted_events())
    }
    pub fn v2_truncated_batches(&self) -> Option<u64> {
        self.v2_dispatcher
            .as_ref()
            .map(|d| d.metrics_truncated_batches())
    }
    pub fn v2_emitted_create(&self) -> Option<u64> {
        self.v2_dispatcher
            .as_ref()
            .map(|d| d.metrics_emitted_create())
    }
    pub fn v2_emitted_update(&self) -> Option<u64> {
        self.v2_dispatcher
            .as_ref()
            .map(|d| d.metrics_emitted_update())
    }
    pub fn v2_emitted_delete(&self) -> Option<u64> {
        self.v2_dispatcher
            .as_ref()
            .map(|d| d.metrics_emitted_delete())
    }
    pub fn v2_queue_drops_total(&self) -> Option<u64> {
        self.v2_dispatcher
            .as_ref()
            .map(|d| d.metrics_queue_drops_total())
    }
    pub fn v2_queue_len(&self) -> Option<u64> {
        self.v2_dispatcher.as_ref().map(|d| d.metrics_queue_len())
    }

    pub async fn debug_raw_keys_for_object(
        &self,
        normalized_id: &str,
    ) -> Result<Vec<Vec<u8>>, ShardError> {
        use crate::granular_key::build_object_prefix;
        use crate::granular_key::parse_granular_key;
        let prefix = build_object_prefix(normalized_id);
        // Compute an upper bound end key by appending 0xFF to prefix for range scan
        let mut end = prefix.clone();
        end.push(0xFF);
        let mut all = Vec::new();
        let (chunk, mut cursor) = self
            .app_storage
            .scan_range_paginated(prefix.as_slice(), end.as_slice(), None)
            .await
            .map_err(ShardError::from)?;
        for (k, _) in chunk {
            if let Some((obj_id, _)) = parse_granular_key(k.as_slice()) {
                if obj_id == normalized_id {
                    all.push(k.into_vec());
                }
            }
        }
        // Drain remaining pages if any
        while let Some(cur) = cursor.take() {
            let start = cur.clone().into_vec();
            let (chunk, next) = self
                .app_storage
                .scan_range_paginated(start.as_slice(), end.as_slice(), None)
                .await
                .map_err(ShardError::from)?;
            for (k, _) in chunk {
                if let Some((obj_id, _)) = parse_granular_key(k.as_slice()) {
                    if obj_id == normalized_id {
                        all.push(k.into_vec());
                    }
                }
            }
            cursor = next;
        }
        all.sort();
        Ok(all)
    }
    /// Create a new ObjectUnifiedShard with full networking and event support
    #[instrument(skip_all, fields(shard_id = %metadata.id))]
    pub async fn new_full(
        metadata: ShardMetadata,
        app_storage: A,
        replication: R,
        z_session: zenoh::Session,
        event_manager: Option<Arc<E>>,
        config: UnifiedShardConfig,
        // Optionally pass V2 dispatcher
        v2_dispatcher: Option<crate::events::V2DispatcherRef>,
    ) -> Result<Self, ShardError> {
        debug!("Creating new full ObjectUnifiedShard");
        let config = Self::sanitize_config(config);
        let (readiness_tx, readiness_rx) = watch::channel(false);
        let metrics = Arc::new(ShardMetrics::new(
            &metadata.collection,
            metadata.partition_id,
        ));

        // Create InvocationOffloader with EventManager if available
        let inv_offloader =
            InvocationOffloader::new(&metadata, event_manager.clone());

        let inv_offloader = Arc::new(inv_offloader);
        let inv_net_manager = InvocationNetworkManager::new(
            z_session.clone(),
            metadata.clone(),
            inv_offloader.clone(),
        );

        let replication_arc = Arc::new(replication);

        Ok(Self {
            metadata,
            app_storage,
            replication: replication_arc,
            metrics,
            readiness_tx,
            readiness_rx,
            z_session: Some(z_session),
            inv_net_manager: Some(Arc::new(Mutex::new(inv_net_manager))),
            inv_offloader: Some(inv_offloader),
            network: OnceCell::new(), // Will be initialized in initialize()
            event_manager,
            liveliness_state: Some(MemberLivelinessState::default()),
            token: CancellationToken::new(),
            config,
            v2_dispatcher,
        })
    }

    /// Create a new ObjectUnifiedShard with minimal components (storage only)
    #[instrument(skip_all, fields(shard_id = %metadata.id))]
    pub async fn new_minimal(
        metadata: ShardMetadata,
        app_storage: A,
        replication: R,
        config: UnifiedShardConfig,
    ) -> Result<Self, ShardError> {
        let config = Self::sanitize_config(config);
        let (readiness_tx, readiness_rx) = watch::channel(false);
        let metrics = Arc::new(ShardMetrics::new(
            &metadata.collection,
            metadata.partition_id,
        ));

        Ok(Self {
            metadata,
            app_storage,
            replication: Arc::new(replication),
            metrics,
            readiness_tx,
            readiness_rx,
            z_session: None,
            inv_net_manager: None,
            inv_offloader: None,
            network: OnceCell::new(), // No network for minimal mode
            event_manager: None,
            liveliness_state: None,
            token: CancellationToken::new(),
            config,
            v2_dispatcher: None,
        })
    }

    #[inline]
    fn sanitize_config(config: UnifiedShardConfig) -> UnifiedShardConfig {
        UnifiedShardConfig {
            granular_prefetch_limit: config.granular_prefetch_limit.max(1),
            ..config
        }
    }

    /// Initialize the shard with an Arc reference to enable network layer
    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]

    /// Get object by ID with event triggering
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    pub async fn get_object(
        &self,
        object_id: &str,
    ) -> Result<Option<ObjectData>, ShardError> {
        // Granular-only mode: reconstruct from per-entry storage.
        self.reconstruct_object_from_entries(
            object_id,
            self.config.granular_prefetch_limit,
        )
        .await
    }

    /// Placeholder for future string-ID get path (Phase 2: signature provided; Phase 3 will implement).
    pub async fn internal_get_object_by_str_id(
        &self,
        normalized_id: &str,
    ) -> Result<Option<ObjectData>, ShardError> {
        // String IDs use the same granular storage; just reconstruct.
        self.reconstruct_object_from_entries(
            normalized_id,
            self.config.granular_prefetch_limit,
        )
        .await
    }

    /// Set object with automatic event triggering
    /// All set operations automatically trigger appropriate events (DataCreate/DataUpdate)
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    pub async fn set_object(
        &self,
        object_id: &str,
        entry: ObjectData,
    ) -> Result<(), ShardError> {
        // Granular-only: decompose ObjectData into per-field entries.
        let normalized_id = object_id.to_string();
        // Cheap existence check via metadata (avoid full reconstruction unless events need diff)
        let metadata_before =
            EntryStore::get_metadata(self, &normalized_id).await?;
        let _is_new = metadata_before.is_none();
        let need_old = self.event_manager.is_some() && entry.event.is_some();
        let _old_entry = if need_old {
            self.reconstruct_object_from_entries(
                &normalized_id,
                self.config.granular_prefetch_limit,
            )
            .await?
        } else {
            None
        };

        // Merge numeric and string maps into a single key map (numeric keys decimal encoded)
        let mut kv: std::collections::HashMap<String, ObjectVal> =
            std::collections::HashMap::with_capacity(entry.entries.len());
        for (k, v) in &entry.entries {
            kv.insert(k.clone(), v.clone());
        }
        // If event config present, persist event config record (0x01) first.
        if let Some(ev_cfg) = &entry.event {
            let key_ev = string_object_event_config_key(&normalized_id);
            let bytes = ev_cfg.encode_to_vec();
            let operation = crate::replication::Operation::Write(
                crate::replication::WriteOperation {
                    key: StorageValue::from(key_ev),
                    value: StorageValue::from(bytes),
                    ..Default::default()
                },
            );
            let request =
                crate::replication::ShardRequest::from_operation(operation, 0);
            self.replication
                .replicate_write(request)
                .await
                .map_err(ShardError::from)?;
        }
        // Batch set all entries (version increment handled inside)
        EntryStore::batch_set_entries(self, &normalized_id, kv, None).await?;

        // TODO: data event triggering removed with legacy helpers cleanup; reintroduce via new event pipeline if needed.
        Ok(())
    }

    pub async fn internal_set_object(
        &self,
        normalized_id: &str,
        entry: ObjectData,
    ) -> Result<(), ShardError> {
        // Granular-only: decompose ObjectData; no event triggers for string IDs yet so skip reconstruction.

        let mut kv: std::collections::HashMap<String, ObjectVal> =
            std::collections::HashMap::with_capacity(entry.entries.len());
        for (k, v) in &entry.entries {
            kv.insert(k.clone(), v.clone());
        }
        if let Some(ev_cfg) = &entry.event {
            let key_ev = string_object_event_config_key(normalized_id);
            let bytes = ev_cfg.encode_to_vec();
            let operation = crate::replication::Operation::Write(
                crate::replication::WriteOperation {
                    key: StorageValue::from(key_ev),
                    value: StorageValue::from(bytes),
                    ..Default::default()
                },
            );
            let request =
                crate::replication::ShardRequest::from_operation(operation, 0);
            self.replication
                .replicate_write(request)
                .await
                .map_err(ShardError::from)?;
        }
        EntryStore::batch_set_entries(self, normalized_id, kv, None).await?;
        Ok(())
    }

    /// Delete object with automatic event triggering
    /// All delete operations automatically trigger DataDelete events
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    pub async fn delete_object(
        &self,
        object_id: &str,
    ) -> Result<(), ShardError> {
        // Use metadata + optional reconstruction only if events are enabled.
        let normalized_id = object_id.to_string();
        let meta_opt = EntryStore::get_metadata(self, &normalized_id).await?;
        if meta_opt.is_none()
            || meta_opt.as_ref().map(|m| m.tombstone).unwrap_or(false)
        {
            return Ok(());
        }

        // Capture existing entries for V2 delete fanout if event pipeline is present
        let existing_entry = if self.v2_dispatcher.is_some() {
            self.reconstruct_object_from_entries(
                &normalized_id,
                self.config.granular_prefetch_limit,
            )
            .await?
        } else {
            None
        };

        // Load event config BEFORE deletion so triggers can fire (delete removes config record)
        let predelete_event_cfg: Option<
            std::sync::Arc<oprc_grpc::ObjectEvent>,
        > = {
            let key_ev = string_object_event_config_key(&normalized_id);
            match self.app_storage.get(&key_ev).await {
                Ok(Some(val)) => oprc_grpc::ObjectEvent::decode(val.as_slice())
                    .ok()
                    .map(std::sync::Arc::new),
                _ => None,
            }
        };

        // Perform bulk delete of all entries
        EntryStore::delete_object_granular(self, &normalized_id).await?;

        // Increment object version and mark tombstone after deletion
        let mut metadata = meta_opt.unwrap_or_default();
        let version_before = metadata.object_version;
        metadata.increment_version();
        metadata.mark_tombstone();
        let version_after = metadata.object_version;
        self.set_metadata(&normalized_id, metadata).await?;

        // Emit V2 per-entry delete events (one per previously existing key)
        if let Some(v2) = &self.v2_dispatcher {
            if let Some(entry) = existing_entry {
                // Collect changed keys from both numeric and string maps
                let mut changed: Vec<ChangedKey> =
                    Vec::with_capacity(entry.entries.len());
                for (k, _v) in entry.entries.iter() {
                    changed.push(ChangedKey {
                        key_canonical: k.clone(),
                        action: MutAction::Delete,
                    });
                }
                if !changed.is_empty() {
                    // Use pre-deletion event config so delete triggers can be evaluated
                    let event_cfg = predelete_event_cfg.clone();
                    let ctx = MutationContext::new(
                        normalized_id.clone(),
                        self.class_id().to_string(),
                        self.partition_id_u16(),
                        version_before,
                        version_after,
                        changed,
                    )
                    .with_event_config(event_cfg);
                    v2.try_send(ctx);
                }
            }
        }

        Ok(())
    }

    /// Delete object by normalized string ID (granular variant).
    pub async fn internal_delete_object(
        &self,
        normalized_id: &str,
    ) -> Result<(), ShardError> {
        // Check metadata; short-circuit if already tombstoned or absent
        let meta_opt = EntryStore::get_metadata(self, normalized_id).await?;
        if meta_opt.is_none()
            || meta_opt.as_ref().map(|m| m.tombstone).unwrap_or(false)
        {
            return Ok(());
        }

        // Capture existing entries for V2 delete fanout if available
        let existing_entry = if self.v2_dispatcher.is_some() {
            self.reconstruct_object_from_entries(
                normalized_id,
                self.config.granular_prefetch_limit,
            )
            .await?
        } else {
            None
        };

        // Load event config BEFORE deletion so triggers can fire (delete removes config record)
        let predelete_event_cfg: Option<
            std::sync::Arc<oprc_grpc::ObjectEvent>,
        > = {
            let key_ev = string_object_event_config_key(normalized_id);
            match self.app_storage.get(&key_ev).await {
                Ok(Some(val)) => oprc_grpc::ObjectEvent::decode(val.as_slice())
                    .ok()
                    .map(std::sync::Arc::new),
                _ => None,
            }
        };

        // Perform bulk delete
        EntryStore::delete_object_granular(self, normalized_id).await?;

        // Increment version and mark tombstone
        let mut metadata = meta_opt.unwrap_or_default();
        let version_before = metadata.object_version;
        metadata.increment_version();
        metadata.mark_tombstone();
        let version_after = metadata.object_version;
        self.set_metadata(normalized_id, metadata).await?;

        // Emit V2 events if dispatcher exists
        if let Some(v2) = &self.v2_dispatcher {
            if let Some(entry) = existing_entry {
                let mut changed: Vec<ChangedKey> =
                    Vec::with_capacity(entry.entries.len());
                for (k, _v) in entry.entries.iter() {
                    changed.push(ChangedKey {
                        key_canonical: k.clone(),
                        action: MutAction::Delete,
                    });
                }
                if !changed.is_empty() {
                    // Use pre-deletion event config so delete triggers can be evaluated
                    let event_cfg = predelete_event_cfg.clone();
                    let ctx = MutationContext::new(
                        normalized_id.to_string(),
                        self.class_id().to_string(),
                        self.partition_id_u16(),
                        version_before,
                        version_after,
                        changed,
                    )
                    .with_event_config(event_cfg);
                    v2.try_send(ctx);
                }
            }
        }

        Ok(())
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, obj_id = normalized_id))]
    pub async fn reconstruct_object_from_entries(
        &self,
        normalized_id: &str,
        prefetch_limit: usize,
    ) -> Result<Option<ObjectData>, ShardError> {
        if prefetch_limit == 0 {
            return Err(ShardError::ConfigurationError(
                "granular prefetch limit must be greater than zero".into(),
            ));
        }

        // First, collect entries (they may replicate before metadata in eventually-consistent setups)
        let mut entries = std::collections::BTreeMap::new();
        let mut cursor: Option<Vec<u8>> = None;
        let mut page_counter: usize = 0;
        const MAX_PAGES: usize = 4096;

        loop {
            page_counter += 1;
            if page_counter > MAX_PAGES {
                return Err(ShardError::ConfigurationError(
                    "granular reconstruction exceeded pagination bounds".into(),
                ));
            }

            let options = EntryListOptions {
                key_prefix: None,
                limit: prefetch_limit,
                cursor: cursor.clone(),
            };

            let page: EntryListResult =
                EntryStore::list_entries(self, normalized_id, options).await?;

            for (key, value) in page.entries {
                entries.insert(key, value);
            }

            match page.next_cursor {
                Some(next_cursor) => {
                    if let Some(current) = &cursor {
                        if current == &next_cursor {
                            return Err(ShardError::ConfigurationError(
                                "granular reconstruction encountered a stalled cursor".
                                    into(),
                            ));
                        }
                    }
                    cursor = Some(next_cursor);
                }
                None => break,
            }
        }

        // If no entries found, fall back to metadata-only existence (empty object)
        let has_any_entries = !entries.is_empty();
        let version = if !has_any_entries {
            let metadata =
                EntryStore::get_metadata(self, normalized_id).await?;
            match metadata {
                Some(meta) if !meta.tombstone => meta.object_version,
                _ => {
                    tracing::trace!(
                        shard_id = %self.metadata.id,
                        obj_id = normalized_id,
                        "reconstruct_object_from_entries: no entries and no live metadata"
                    );
                    return Ok(None);
                }
            }
        } else {
            // Read metadata (may arrive after entries). If missing, synthesize default version.
            let metadata =
                EntryStore::get_metadata(self, normalized_id).await?;
            match metadata {
                Some(meta) if !meta.tombstone => meta.object_version,
                Some(meta) if meta.tombstone => return Ok(None),
                _ => 0,
            }
        };

        let entry = ObjectData {
            last_updated: version,
            entries,
            event: None,
        };
        tracing::trace!(
            shard_id = %self.metadata.id,
            obj_id = normalized_id,
            version = version,
            n_entries = entry.entries.len(),
            "reconstruct_object_from_entries: returning object"
        );
        Ok(Some(entry))
    }

    /// Get count of objects
    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    pub async fn count_objects(&self) -> Result<usize, ShardError> {
        // Count metadata records (one per object) in granular storage
        let mut total = 0usize;
        let results = self
            .app_storage
            .scan(&[]) // full scan
            .await
            .map_err(ShardError::from)?;
        for (k, _v) in results {
            if let Some((_oid, crate::granular_key::GranularRecord::Metadata)) =
                crate::granular_key::parse_granular_key(k.as_slice())
            {
                total += 1;
            }
        }
        Ok(total)
    }
    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    fn sync_network(&self) {
        // Clone the components we need for the async task
        let metadata = self.metadata.clone();
        let readiness_rx = self.readiness_rx.clone();
        let inv_net_manager = self.inv_net_manager.clone();
        let liveliness_state = self.liveliness_state.clone();
        let token = self.token.clone();

        if let Some(session) = &self.z_session {
            let session = session.clone();
            tokio::spawn(async move {
                let mut receiver = readiness_rx;
                let liveliness_selector = format!(
                    "oprc/{}/{}/liveliness/*",
                    metadata.collection, metadata.partition_id
                );
                let liveliness_sub = match session
                    .liveliness()
                    .declare_subscriber(liveliness_selector)
                    .await
                {
                    Ok(sub) => sub,
                    Err(e) => {
                        error!(
                            "Failed to declare liveliness subscriber for shard {}: {}",
                            metadata.id, e
                        );
                        return;
                    }
                };
                loop {
                    tokio::select! {
                        res = receiver.changed() => {
                            if let Err(_e) = res { break; }
                            info!("Shard {} readiness: {}", metadata.id, receiver.borrow().to_owned());
                        }
                        token = liveliness_sub.recv_async() => {
                            match token {
                                Ok(sample) => {
                                    if let Some(liveliness) = &liveliness_state {
                                        let id = liveliness.handle_sample(&sample).await;
                                        info!("shard {}: liveliness {id:?} updated {sample:?}", metadata.id );
                                        if id != Some(metadata.id) {
                                            if let Some(inv_manager) = &inv_net_manager {
                                                let mut inv = inv_manager.lock().await;
                                                inv.on_liveliness_updated(liveliness).await;
                                            }
                                        }
                                    }
                                },
                                Err(_) => {},
                            };
                        }
                        _ = token.cancelled() => { break; }
                    }
                }
                info!("shard {}: Network sync loop exited", metadata.id);
            });
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    pub async fn close(&self) -> Result<(), ShardError> {
        info!("Closing shard");
        self.token.cancel();

        // Stop invocation manager if available
        if let Some(inv_manager) = &self.inv_net_manager {
            inv_manager.lock().await.stop().await;
        }

        Ok(())
    }

    /// Trigger an event if event manager is available
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id = context.object_id))]
    pub async fn trigger_event(&self, context: EventContext) {
        if let Some(event_manager) = &self.event_manager {
            // For unified shard, we need to get the object entry first
            if let Ok(Some(entry)) = self.get_object(&context.object_id).await {
                event_manager
                    .trigger_event_with_entry(context, &entry)
                    .await;
            } else {
                // If we can't get the entry, we can still try to trigger the event
                // but the event manager might not have full context
                tracing::warn!(
                    "Could not retrieve object {} for event triggering",
                    context.object_id
                );
            }
        }
    }

    /// Trigger event with object entry already available (more efficient)
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id = context.object_id))]
    pub async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectData,
    ) {
        if let Some(event_manager) = &self.event_manager {
            event_manager
                .trigger_event_with_entry(context, object_entry)
                .await;
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, obj_id))]
    pub async fn ensure_metadata_exists(
        &self,
        obj_id: &str,
    ) -> Result<bool, ShardError> {
        // Fast path: if metadata exists (including tombstone), do not overwrite to avoid resurrection.
        if let Some(meta) = EntryStore::get_metadata(self, obj_id).await? {
            if meta.tombstone {
                return Ok(false); // do not resurrect tombstoned object here
            }
            return Ok(false); // already exists
        }
        // Attempt replicated write of default metadata; race window acceptable—second writer sees overwrite flag.
        let meta = ObjectMetadata::default();
        let key = crate::granular_key::build_metadata_key(obj_id);
        let value = oprc_dp_storage::StorageValue::from(meta.to_bytes());
        let operation = crate::replication::Operation::Write(
            crate::replication::WriteOperation {
                key: oprc_dp_storage::StorageValue::from(key.clone()),
                value: value.clone(),
                return_old: true, // request overwrite info
                ..Default::default()
            },
        );
        let request =
            crate::replication::ShardRequest::from_operation(operation, 0);
        let resp = self
            .replication
            .replicate_write(request)
            .await
            .map_err(ShardError::from)?;
        // OperationExtra::Write(overrode) => overrode==false means we created, true means someone else beat us.
        match resp.extra {
            crate::replication::OperationExtra::Write(overrode) => {
                Ok(!overrode)
            }
            _ => Ok(false),
        }
    }
}

/// Serialize ObjectEntry to StorageValue
fn serialize_object_entry(
    entry: &ObjectData,
) -> Result<StorageValue, ShardError> {
    match bincode::serde::encode_to_vec(entry, bincode::config::standard()) {
        Ok(bytes) => Ok(StorageValue::from(bytes)),
        Err(e) => Err(ShardError::SerializationError(format!(
            "Failed to serialize ObjectEntry: {}",
            e
        ))),
    }
}

/// Deserialize StorageValue to ObjectEntry
fn deserialize_object_entry(
    storage_value: &StorageValue,
) -> Result<ObjectData, ShardError> {
    let bytes = storage_value.as_slice();
    match bincode::serde::decode_from_slice(bytes, bincode::config::standard())
    {
        Ok((entry, _)) => Ok(entry), // bincode v2 returns (T, bytes_read)
        Err(e) => Err(ShardError::SerializationError(format!(
            "Failed to deserialize ObjectEntry: {}",
            e
        ))),
    }
}

// Implement the UnifiedObjectShard trait
#[async_trait::async_trait]
impl<A, R, E> ObjectShard for ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
    E: EventManager + Send + Sync + 'static,
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn meta(&self) -> &ShardMetadata {
        &self.metadata
    }

    async fn get_object_by_str_id(
        &self,
        normalized_id: &str,
    ) -> Result<Option<ObjectData>, ShardError> {
        self.internal_get_object_by_str_id(normalized_id).await
    }

    fn watch_readiness(&self) -> watch::Receiver<bool> {
        self.readiness_rx.clone()
    }

    async fn initialize(&self) -> Result<(), ShardError> {
        // Initialize replication layer first
        self.replication.initialize().await?;

        // Create network layer if Zenoh session exists
        if let Some(z_session) = &self.z_session {
            let prefix = format!(
                "oprc/{}/{}/objects",
                self.metadata.collection, self.metadata.partition_id
            );
            let network = UnifiedShardNetwork::new(
                z_session.clone(),
                self.replication.clone(),
                self.metadata.clone(),
                prefix,
                self.config.max_string_id_len,
            );
            let network_arc = Arc::new(Mutex::new(network));
            // Initialize network once (silently ignore if already set)
            let _ = self.network.set(network_arc);
        }

        // Storage backend initialization is implicit
        // Watch replication readiness and forward to shard readiness
        let repl_readiness = self.replication.watch_readiness();
        let shard_readiness_tx = self.readiness_tx.clone();
        let _ = shard_readiness_tx.send(*repl_readiness.borrow());

        let token = self.token.clone();

        tokio::spawn(async move {
            let mut repl_rx = repl_readiness;
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    res = repl_rx.changed() => {
                        if res.is_err() {
                            break;
                        }
                        let is_ready = *repl_rx.borrow();
                        let _ = shard_readiness_tx.send(is_ready);
                    }
                }
            }
        });

        // Initialize networking components if available
        if let Some(inv_manager) = &self.inv_net_manager {
            inv_manager
                .lock()
                .await
                .start()
                .await
                .map_err(|e| ShardError::OdgmError(OdgmError::ZenohError(e)))?;
        }

        // Do not start network here; it will be started after the shard is wrapped in Arc

        info!("Shard {} initialization complete", self.metadata.id);
        Ok(())
    }

    async fn close(&self) -> Result<(), ShardError> {
        self.close().await
    }

    async fn get_object(
        &self,
        object_id: &str,
    ) -> Result<Option<ObjectData>, ShardError> {
        self.get_object(object_id).await
    }

    #[inline]
    async fn set_object(
        &self,
        object_id: &str,
        entry: ObjectData,
    ) -> Result<(), ShardError> {
        self.set_object(object_id, entry).await
    }

    #[inline]
    async fn set_object_by_str_id(
        &self,
        normalized_id: &str,
        entry: ObjectData,
    ) -> Result<(), ShardError> {
        self.internal_set_object(normalized_id, entry).await
    }

    async fn delete_object(&self, object_id: &str) -> Result<(), ShardError> {
        self.delete_object(object_id).await
    }

    async fn delete_object_by_str_id(
        &self,
        normalized_id: &str,
    ) -> Result<(), ShardError> {
        self.internal_delete_object(normalized_id).await
    }

    async fn count_objects(&self) -> Result<usize, ShardError> {
        self.count_objects().await
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn scan_objects(
        &self,
        prefix: Option<&str>,
    ) -> Result<Vec<(String, ObjectData)>, ShardError> {
        let mut out = Vec::new();
        // Granular scan: list all keys, filter for metadata keys (suffix 0x00), reconstruct.
        // This is expensive; intended for migration/debug.
        let all = self.app_storage.scan(&[]).await?;
        for (k, _v) in all {
            if let Some((
                oid_str,
                crate::granular_key::GranularRecord::Metadata,
            )) = crate::granular_key::parse_granular_key(k.as_slice())
            {
                if let Some(pref) = prefix {
                    if !oid_str.starts_with(pref) {
                        continue;
                    }
                }
                if let Some(entry) = self
                    .reconstruct_object_from_entries(
                        &oid_str,
                        self.config.granular_prefetch_limit,
                    )
                    .await?
                {
                    out.push((oid_str, entry));
                }
            }
        }
        Ok(out)
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, count = entries.len()))]
    async fn batch_set_objects(
        &self,
        entries: Vec<(String, ObjectData)>,
    ) -> Result<(), ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for (key, entry) in entries {
            self.set_object(&key, entry).await?;
        }
        Ok(())
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, count = keys.len()))]
    async fn batch_delete_objects(
        &self,
        keys: Vec<String>,
    ) -> Result<(), ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for key in keys {
            self.delete_object(&key).await?;
        }
        Ok(())
    }

    async fn get_metadata_granular(
        &self,
        normalized_id: &str,
    ) -> Result<Option<ObjectMetadata>, ShardError> {
        EntryStore::get_metadata(self, normalized_id).await
    }

    #[inline]
    async fn set_metadata_granular(
        &self,
        normalized_id: &str,
        metadata: ObjectMetadata,
    ) -> Result<(), ShardError> {
        EntryStore::set_metadata(self, normalized_id, metadata).await
    }

    async fn get_entry_granular(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<Option<ObjectVal>, ShardError> {
        EntryStore::get_entry(self, normalized_id, key).await
    }

    async fn set_entry_granular(
        &self,
        normalized_id: &str,
        key: &str,
        value: ObjectVal,
    ) -> Result<(), ShardError> {
        EntryStore::set_entry(self, normalized_id, key, value).await
    }

    async fn delete_entry_granular(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<(), ShardError> {
        EntryStore::delete_entry(self, normalized_id, key).await
    }

    async fn list_entries_granular(
        &self,
        normalized_id: &str,
        options: EntryListOptions,
    ) -> Result<EntryListResult, ShardError> {
        EntryStore::list_entries(self, normalized_id, options).await
    }

    async fn batch_set_entries_granular(
        &self,
        normalized_id: &str,
        values: HashMap<String, ObjectVal>,
        expected_version: Option<u64>,
    ) -> Result<u64, ShardError> {
        EntryStore::batch_set_entries(
            self,
            normalized_id,
            values,
            expected_version,
        )
        .await
    }

    async fn batch_delete_entries_granular(
        &self,
        normalized_id: &str,
        keys: Vec<String>,
    ) -> Result<(), ShardError> {
        EntryStore::batch_delete_entries(self, normalized_id, keys).await
    }

    async fn reconstruct_object_granular(
        &self,
        normalized_id: &str,
        prefetch_limit: usize,
    ) -> Result<Option<ObjectData>, ShardError> {
        self.reconstruct_object_from_entries(normalized_id, prefetch_limit)
            .await
    }

    async fn ensure_metadata_exists(
        &self,
        normalized_id: &str,
    ) -> Result<bool, ShardError> {
        self.ensure_metadata_exists(normalized_id).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn UnifiedShardTransaction + '_>, ShardError> {
        match self.app_storage.begin_write_transaction() {
            Ok(storage_tx) => {
                let tx = UnifiedShardWriteTxAdapter {
                    tx: Some(storage_tx),
                    metrics: self.metrics.clone(),
                    completed: false,
                };
                Ok(Box::new(tx))
            }
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    async fn trigger_event(&self, context: EventContext) {
        self.trigger_event(context).await;
    }

    async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectData,
    ) {
        self.trigger_event_with_entry(context, object_entry).await;
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn invoke_fn(
        &self,
        req: InvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        if let Some(offloader) = &self.inv_offloader {
            offloader.invoke_fn(req).await
        } else {
            Err(OffloadError::ConfigurationError(
                "Invocation offloader not available".to_string(),
            ))
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn invoke_obj(
        &self,
        req: ObjectInvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        if let Some(offloader) = &self.inv_offloader {
            offloader.invoke_obj(req).await
        } else {
            Err(OffloadError::ConfigurationError(
                "Invocation offloader not available".to_string(),
            ))
        }
    }

    async fn start_network(
        &self,
        arc_self: ArcUnifiedObjectShard,
    ) -> Result<(), ShardError> {
        if let Some(network_arc) = self.network.get() {
            let mut network = network_arc.lock().await;
            // Attach shard so handlers can reconstruct via object_api
            network.attach_shard(arc_self);
            if !network.is_running() {
                network.start().await.map_err(|e| {
                    ShardError::OdgmError(OdgmError::ZenohError(
                        format!("Failed to start network: {}", e).into(),
                    ))
                })?;
            }
        }
        // Initialize liveliness (token declaration + subscription loop)
        if let (Some(session), Some(liveliness)) =
            (&self.z_session, &self.liveliness_state)
        {
            // Declare our own liveliness token so others can observe us
            liveliness.declare_liveliness(session, &self.metadata).await;
            // Start subscriber loop to track other members (only once)
            self.sync_network();
            tracing::info!(shard_id = %self.metadata.id, "Liveliness initialized for shard");
        } else {
            tracing::debug!(shard_id = %self.metadata.id, "Liveliness skipped: missing session or state");
        }
        Ok(())
    }
}

// Implement the IntoUnifiedShard trait for easy conversion
impl<A, R, E> IntoUnifiedShard for ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
    E: EventManager + Send + Sync + 'static,
{
    fn into_boxed(self) -> BoxedUnifiedObjectShard {
        Box::new(self)
    }

    fn into_arc(self) -> ArcUnifiedObjectShard {
        Arc::new(self)
    }
}

// Adapter to wrap application write transactions into UnifiedShardTransaction
struct UnifiedShardWriteTxAdapter<T> {
    tx: Option<T>,
    metrics: Arc<ShardMetrics>,
    completed: bool,
}

#[async_trait::async_trait(?Send)]
impl<T> UnifiedShardTransaction for UnifiedShardWriteTxAdapter<T>
where
    T: oprc_dp_storage::ApplicationWriteTransaction<
            Error = oprc_dp_storage::StorageError,
        >,
{
    async fn get(&self, key: &u64) -> Result<Option<ObjectData>, ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match &self.tx {
            Some(tx) => {
                let key_bytes = key.to_be_bytes();
                match tx.get(&key_bytes).await {
                    Ok(Some(data)) => {
                        let entry = deserialize_object_entry(&data)?;
                        Ok(Some(entry))
                    }
                    Ok(None) => Ok(None),
                    Err(e) => Err(ShardError::StorageError(e)),
                }
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn set(
        &mut self,
        key: u64,
        entry: ObjectData,
    ) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match &mut self.tx {
            Some(tx) => {
                let key_bytes = key.to_be_bytes();
                let value = serialize_object_entry(&entry)?;
                tx.put(&key_bytes, value)
                    .await
                    .map_err(ShardError::StorageError)
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn delete(&mut self, key: &u64) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match &mut self.tx {
            Some(tx) => {
                let key_bytes = key.to_be_bytes();
                tx.delete(&key_bytes)
                    .await
                    .map_err(ShardError::StorageError)
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match self.tx.take() {
            Some(tx) => tx.commit().await.map_err(|e| {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ShardError::StorageError(e)
            }),
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match self.tx.take() {
            Some(tx) => tx.rollback().await.map_err(ShardError::StorageError),
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }
}
