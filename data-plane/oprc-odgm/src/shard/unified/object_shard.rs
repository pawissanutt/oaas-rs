use std::sync::Arc;
use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument};

use super::config::{ShardError, ShardMetrics};
use super::network::UnifiedShardNetwork;
use super::object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard,
    ObjectShard, UnifiedShardTransaction,
};
use super::traits::ShardMetadata;
use crate::error::OdgmError;
use crate::events::{EventContext, EventManager};
use crate::replication::{
    DeleteOperation, Operation, OperationExtra, ReplicationLayer,
    ResponseStatus, ShardRequest, WriteOperation,
};
use crate::shard::{
    ObjectEntry,
    invocation::{InvocationNetworkManager, InvocationOffloader},
    liveliness::MemberLivelinessState,
};
use oprc_dp_storage::{ApplicationDataStorage, StorageValue};
use oprc_invoke::OffloadError;
use oprc_pb::{InvocationRequest, InvocationResponse, ObjectInvocationRequest};

/// Unified ObjectShard that combines storage, networking, events, and management
pub struct ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage,
    R: ReplicationLayer,
    E: EventManager + Send + Sync + 'static,
{
    // Core storage and metadata (merged from UnifiedShard)
    metadata: ShardMetadata,
    app_storage: A, // Direct access to application storage - log storage is now part of replication
    replication: Arc<R>, // Mandatory replication layer (use NoReplication for single-node)
    metrics: Arc<ShardMetrics>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,

    // Networking components (optional - can be disabled for storage-only use)
    z_session: Option<zenoh::Session>,
    pub(crate) inv_net_manager: Option<Arc<Mutex<InvocationNetworkManager<E>>>>,
    pub(crate) inv_offloader: Option<Arc<InvocationOffloader<E>>>,
    pub(crate) network: Option<Arc<Mutex<UnifiedShardNetwork<R>>>>,

    // Event management (optional)
    event_manager: Option<Arc<E>>,

    // Liveliness management (optional)
    liveliness_state: Option<MemberLivelinessState>,

    // Control and metadata
    token: CancellationToken,
}

impl<A, R, E> ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
    E: EventManager + Send + Sync + 'static,
{
    /// Create a new ObjectUnifiedShard with full networking and event support
    #[instrument(skip_all, fields(shard_id = %metadata.id))]
    pub async fn new_full(
        metadata: ShardMetadata,
        app_storage: A,
        replication: R,
        z_session: zenoh::Session,
        event_manager: Option<Arc<E>>,
    ) -> Result<Self, ShardError> {
        debug!("Creating new full ObjectUnifiedShard");
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

        // Create modernized unified network layer
        let prefix = format!(
            "oprc/{}/{}/objects",
            metadata.collection, metadata.partition_id
        );
        let replication_arc = Arc::new(replication);
        let network = UnifiedShardNetwork::new(
            z_session.clone(),
            replication_arc.clone(),
            metadata.clone(),
            prefix,
        );

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
            network: Some(Arc::new(Mutex::new(network))),
            event_manager,
            liveliness_state: Some(MemberLivelinessState::default()),
            token: CancellationToken::new(),
        })
    }

    /// Create a new ObjectUnifiedShard with minimal components (storage only)
    #[instrument(skip_all, fields(shard_id = %metadata.id))]
    pub async fn new_minimal(
        metadata: ShardMetadata,
        app_storage: A,
        replication: R,
    ) -> Result<Self, ShardError> {
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
            network: None, // Will be set in initialize()
            event_manager: None,
            liveliness_state: None,
            token: CancellationToken::new(),
        })
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    pub async fn initialize(&self) -> Result<(), ShardError> {
        // Initialize replication layer first
        self.replication.initialize().await?;

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

        // Set initial readiness based on replication layer
        let initial_ready = *self.replication.watch_readiness().borrow();
        let _ = self.readiness_tx.send(initial_ready);

        // Initialize networking components if available
        if let Some(inv_manager) = &self.inv_net_manager {
            inv_manager
                .lock()
                .await
                .start()
                .await
                .map_err(|e| ShardError::OdgmError(OdgmError::ZenohError(e)))?;
        }

        // Initialize modern unified network layer
        if let Some(network_arc) = &self.network {
            let mut network = network_arc.lock().await;

            // Start the network layer
            if let Err(e) = network.start().await {
                error!("Failed to start unified network layer: {}", e);
                return Err(ShardError::OdgmError(OdgmError::ZenohError(
                    format!("Network start error: {}", e).into(),
                )));
            }
        }

        // Start network sync if available
        if self.z_session.is_some() {
            self.sync_network();
        }

        // Declare liveliness if available
        if let (Some(session), Some(liveliness)) =
            (&self.z_session, &self.liveliness_state)
        {
            liveliness.declare_liveliness(session, &self.metadata).await;
            liveliness.update(session, &self.metadata).await;
        }

        Ok(())
    }

    /// Get object by ID with event triggering
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    pub async fn get_object(
        &self,
        object_id: u64,
    ) -> Result<Option<ObjectEntry>, ShardError> {
        let key = object_id.to_be_bytes();
        match self.get_storage_value(&key).await? {
            Some(storage_value) => {
                let entry = deserialize_object_entry(&storage_value)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// Set object with automatic event triggering
    /// All set operations automatically trigger appropriate events (DataCreate/DataUpdate)
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    pub async fn set_object(
        &self,
        object_id: u64,
        entry: ObjectEntry,
    ) -> Result<(), ShardError> {
        let key = object_id.to_be_bytes();
        let storage_value = serialize_object_entry(&entry)?;

        if entry.event.is_some() {
            // Perform the storage operation and get info about whether it was new
            let old_storage_value = self
                .set_storage_value_with_return(&key, storage_value)
                .await?;

            // Convert old storage value to ObjectEntry if needed for events
            let old_entry = if let Some(old_val) = old_storage_value {
                Some(deserialize_object_entry(&old_val)?)
            } else {
                None
            };

            // Automatically trigger events if event manager is available
            if self.event_manager.is_some() {
                self.trigger_data_events(
                    object_id,
                    &entry,
                    old_entry.as_ref(),
                    old_entry.is_none(),
                )
                .await;
            }
        } else {
            self.set_storage_value(&key, storage_value).await?;
        }
        Ok(())
    }

    /// Delete object with automatic event triggering
    /// All delete operations automatically trigger DataDelete events
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    pub async fn delete_object(
        &self,
        object_id: &u64,
    ) -> Result<(), ShardError> {
        let key = object_id.to_be_bytes();

        // Perform the deletion and get the deleted entry for event purposes
        let deleted_storage_value = self.delete_storage_value(&key).await?;

        // Convert deleted storage value to ObjectEntry for events
        let deleted_entry = if let Some(old_val) = deleted_storage_value {
            Some(deserialize_object_entry(&old_val)?)
        } else {
            None
        };

        // Automatically trigger delete events if available
        if let (Some(_), Some(entry)) = (&self.event_manager, deleted_entry) {
            self.trigger_delete_events(*object_id, &entry).await;
        }

        Ok(())
    }

    /// Get count of objects
    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    pub async fn count_objects(&self) -> Result<usize, ShardError> {
        let count = self.count_storage_values().await?;
        Ok(count as usize)
    }

    /// Internal storage operations that handle replication
    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn get_storage_value(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, ShardError> {
        // All replication types can read from local app storage
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match self.app_storage.get(key).await {
            Ok(Some(data)) => Ok(Some(data)),
            Ok(None) => Ok(None),
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn set_storage_value(
        &self,
        key: &[u8],
        entry: StorageValue,
    ) -> Result<bool, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // entry is already a StorageValue containing serialized bytes - no need to serialize again
        match self.replication.as_ref() {
            repl => {
                // Route through replication layer
                let operation = Operation::Write(WriteOperation {
                    key: StorageValue::from(key.to_vec()),
                    value: entry, // Use the StorageValue directly
                    ..Default::default()
                });

                let request =
                    ShardRequest::from_operation(operation, self.metadata.id);

                let response = repl.replicate_write(request).await?;
                match &response.status {
                    ResponseStatus::Applied => {
                        Ok(response.extra == OperationExtra::Write(true))
                    }
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(crate::replication::ReplicationError::ConsensusError(reason.clone())))
                    }
                    _ => Err(ShardError::ReplicationError(
                        crate::replication::ReplicationError::ConsensusError("Unknown response".to_string()),
                    )),
                }
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn set_storage_value_with_return(
        &self,
        key: &[u8],
        entry: StorageValue,
    ) -> Result<Option<StorageValue>, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // entry is already a StorageValue containing serialized bytes - no need to serialize again
        match self.replication.as_ref() {
            repl => {
                // Route through replication layer
                let operation = Operation::Write(WriteOperation {
                    key: StorageValue::from(key.to_vec()),
                    value: entry, // Use the StorageValue directly
                    return_old: true,
                    ..Default::default()
                });

                let request =
                    ShardRequest::from_operation(operation, self.metadata.id);

                let response = repl.replicate_write(request).await?;
                match &response.status {
                    ResponseStatus::Applied => Ok(response.data),
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(crate::replication::ReplicationError::ConsensusError(reason.clone())))
                    }
                    _ => Err(ShardError::ReplicationError(
                        crate::replication::ReplicationError::ConsensusError("Unknown response".to_string()),
                    )),
                }
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn delete_storage_value(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // First get the old value for event purposes
        let old_value = self.get_storage_value(key).await?;

        match self.replication.as_ref() {
            repl => {
                let operation = Operation::Delete(DeleteOperation {
                    key: key.to_vec().into(),
                });

                let request =
                    ShardRequest::from_operation(operation, self.metadata.id);

                let response = repl.replicate_write(request).await?;

                match response.status {
                    ResponseStatus::Applied => Ok(old_value),
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(crate::replication::ReplicationError::ConsensusError(reason)))
                    }
                    _ => Err(ShardError::ReplicationError(
                        crate::replication::ReplicationError::ConsensusError("Unknown response".to_string()),
                    )),
                }
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn count_storage_values(&self) -> Result<u64, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match self.app_storage.count().await {
            Ok(count) => Ok(count),
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
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
                            if let Err(e) = res {
                                error!("Failed to receive readiness change: {}", e);
                                break;
                            }

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
                        _ = token.cancelled() => {
                            break;
                        }
                    }
                }
                info!("shard {}: Network sync loop exited", metadata.id);
            });
        }
    }

    // Event management methods
    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id, is_new))]
    async fn trigger_data_events(
        &self,
        object_id: u64,
        new_entry: &ObjectEntry,
        old_entry: Option<&ObjectEntry>,
        is_new: bool,
    ) {
        use crate::events::types::EventType;
        tracing::debug!("Triggering data events for object");

        if let Some(event_manager) = &self.event_manager {
            // Only trigger events if the new entry has events configured
            if let Some(event) = &new_entry.event {
                let meta = &self.metadata;

                // Compare fields and trigger appropriate events
                for (field_id, new_val) in &new_entry.value {
                    if !event.data_trigger.contains_key(field_id) {
                        continue;
                    }
                    let triggered = event.data_trigger.get(field_id).unwrap();
                    if triggered.on_create.is_empty()
                        && triggered.on_update.is_empty()
                    {
                        continue;
                    }

                    let event_type = if is_new {
                        EventType::DataCreate(*field_id)
                    } else if old_entry
                        .and_then(|e| e.value.get(field_id))
                        .map(|old_val| old_val != new_val)
                        .unwrap_or(true)
                    {
                        EventType::DataUpdate(*field_id)
                    } else {
                        continue; // No change
                    };

                    let context = EventContext {
                        object_id,
                        class_id: meta.collection.clone(),
                        partition_id: meta.partition_id,
                        event_type,
                        payload: Some(new_val.data.clone()),
                        error_message: None,
                    };

                    // Trigger event through the event manager
                    event_manager
                        .trigger_event_with_entry(context, new_entry)
                        .await;
                }
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, object_id))]
    async fn trigger_delete_events(
        &self,
        object_id: u64,
        deleted_entry: &ObjectEntry,
    ) {
        use crate::events::types::EventType;

        if let Some(event_manager) = &self.event_manager {
            // Only trigger events if the deleted entry had events configured
            if let Some(event) = &deleted_entry.event {
                let meta = &self.metadata;

                for field_id in deleted_entry.value.keys() {
                    if !event.data_trigger.contains_key(field_id) {
                        continue;
                    };

                    let context = EventContext {
                        object_id,
                        class_id: meta.collection.clone(),
                        partition_id: meta.partition_id,
                        event_type: EventType::DataDelete(*field_id),
                        payload: None,
                        error_message: None,
                    };

                    // Trigger event through the event manager
                    event_manager
                        .trigger_event_with_entry(context, deleted_entry)
                        .await;
                }
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    pub async fn close(self) -> Result<(), ShardError> {
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
            if let Ok(Some(entry)) = self.get_object(context.object_id).await {
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
        object_entry: &ObjectEntry,
    ) {
        if let Some(event_manager) = &self.event_manager {
            event_manager
                .trigger_event_with_entry(context, object_entry)
                .await;
        }
    }
}

/// Serialize ObjectEntry to StorageValue
fn serialize_object_entry(
    entry: &ObjectEntry,
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
) -> Result<ObjectEntry, ShardError> {
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
    fn meta(&self) -> &ShardMetadata {
        &self.metadata
    }

    fn watch_readiness(&self) -> watch::Receiver<bool> {
        self.readiness_rx.clone()
    }

    async fn initialize(&self) -> Result<(), ShardError> {
        self.initialize().await
    }

    async fn close(self: Box<Self>) -> Result<(), ShardError> {
        (*self).close().await
    }

    async fn get_object(
        &self,
        object_id: u64,
    ) -> Result<Option<ObjectEntry>, ShardError> {
        self.get_object(object_id).await
    }

    async fn set_object(
        &self,
        object_id: u64,
        entry: ObjectEntry,
    ) -> Result<(), ShardError> {
        self.set_object(object_id, entry).await
    }

    async fn delete_object(&self, object_id: &u64) -> Result<(), ShardError> {
        self.delete_object(object_id).await
    }

    async fn count_objects(&self) -> Result<usize, ShardError> {
        self.count_objects().await
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id))]
    async fn scan_objects(
        &self,
        prefix: Option<&u64>,
    ) -> Result<Vec<(u64, ObjectEntry)>, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let prefix_bytes = prefix.map(|p| p.to_be_bytes()).unwrap_or([0u8; 8]);
        let prefix_slice = if prefix.is_some() {
            &prefix_bytes[..]
        } else {
            &[]
        };
        match self.app_storage.scan(prefix_slice).await {
            Ok(results) => {
                let mut converted = Vec::new();
                for (key_val, value_val) in results {
                    // Convert 8-byte key back to u64
                    if key_val.len() == 8 {
                        let key_array: [u8; 8] =
                            key_val.as_slice().try_into().unwrap_or([0; 8]);
                        let key = u64::from_be_bytes(key_array);
                        let entry = deserialize_object_entry(&value_val)?;
                        converted.push((key, entry));
                    }
                }
                Ok(converted)
            }
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, count = entries.len()))]
    async fn batch_set_objects(
        &self,
        entries: Vec<(u64, ObjectEntry)>,
    ) -> Result<(), ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for (key, entry) in entries {
            self.set_object(key, entry).await?;
        }
        Ok(())
    }

    #[instrument(skip_all, fields(shard_id = %self.metadata.id, count = keys.len()))]
    async fn batch_delete_objects(
        &self,
        keys: Vec<u64>,
    ) -> Result<(), ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for key in keys {
            self.delete_object(&key).await?;
        }
        Ok(())
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
        object_entry: &ObjectEntry,
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
    async fn get(&self, key: &u64) -> Result<Option<ObjectEntry>, ShardError> {
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
        entry: ObjectEntry,
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
