use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::config::{ShardError, ShardMetrics};
use super::object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard,
    UnifiedObjectShard, UnifiedShardTransaction,
};
use super::traits::{ShardMetadata, ShardTransaction};
use crate::error::OdgmError;
use crate::events::{EventContext, EventManager};
use crate::replication::{
    DeleteOperation, Operation, ReplicationLayer, ResponseStatus, ShardRequest,
    WriteOperation,
};
use crate::shard::{
    invocation::{InvocationNetworkManager, InvocationOffloader},
    liveliness::MemberLivelinessState,
    network::ShardNetwork,
    ObjectEntry, ShardState as LegacyShardState,
};
use oprc_dp_storage::{
    ApplicationDataStorage, StorageTransaction, StorageValue,
};

/// Unified ObjectShard that combines storage, networking, events, and management
pub struct ObjectUnifiedShard<A, R>
where
    A: ApplicationDataStorage,
    R: ReplicationLayer,
{
    // Core storage and metadata (merged from UnifiedShard)
    metadata: ShardMetadata,
    app_storage: A, // Direct access to application storage - log storage is now part of replication
    replication: R, // Mandatory replication layer (use NoReplication for single-node)
    metrics: Arc<ShardMetrics>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,

    // Networking components (optional - can be disabled for storage-only use)
    z_session: Option<zenoh::Session>,
    pub(crate) inv_net_manager: Option<Arc<Mutex<InvocationNetworkManager>>>,
    pub(crate) inv_offloader: Option<Arc<InvocationOffloader>>,
    pub(crate) network: Option<Arc<Mutex<ShardNetwork>>>,

    // Event management (optional)
    event_manager: Option<Arc<EventManager>>,

    // Liveliness management (optional)
    liveliness_state: Option<MemberLivelinessState>,

    // Control and metadata
    token: CancellationToken,
    legacy_metadata: crate::shard::ShardMetadata,
}

impl<A, R> ObjectUnifiedShard<A, R>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + Default + 'static,
{
    /// Create a new ObjectUnifiedShard with full networking and event support
    pub async fn new_full(
        metadata: ShardMetadata,
        app_storage: A,
        replication: R,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<Self, ShardError> {
        let (readiness_tx, readiness_rx) = watch::channel(false);
        let metrics = Arc::new(ShardMetrics::new(
            &metadata.collection,
            metadata.partition_id,
        ));
        let legacy_metadata = metadata.clone().into();

        // Set up networking components
        let inv_offloader =
            Arc::new(InvocationOffloader::new(&legacy_metadata));
        let inv_net_manager = InvocationNetworkManager::new(
            z_session.clone(),
            legacy_metadata.clone(),
            inv_offloader.clone(),
        );

        // Create a basic shard for network compatibility (temporary)
        use crate::shard::BasicObjectShard;
        let basic_shard =
            Arc::new(BasicObjectShard::new(legacy_metadata.clone()))
                as Arc<dyn LegacyShardState<Key = u64, Entry = ObjectEntry>>;
        let prefix = format!(
            "oprc/{}/{}/objects",
            metadata.collection, metadata.partition_id
        );
        let network = ShardNetwork::new(z_session.clone(), basic_shard, prefix);

        Ok(Self {
            metadata,
            app_storage,
            replication,
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
            legacy_metadata,
        })
    }

    /// Create a new ObjectUnifiedShard with minimal components (storage only)
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
        let legacy_metadata = metadata.clone().into();

        Ok(Self {
            metadata,
            app_storage,
            replication,
            metrics,
            readiness_tx,
            readiness_rx,
            z_session: None,
            inv_net_manager: None,
            inv_offloader: None,
            network: None,
            event_manager: None,
            liveliness_state: None,
            token: CancellationToken::new(),
            legacy_metadata,
        })
    }

    pub async fn initialize(&self) -> Result<(), ShardError> {
        // Storage backend initialization is implicit
        // Watch replication readiness and forward to shard readiness
        let repl_readiness = self.replication.watch_readiness();
        let shard_readiness_tx = self.readiness_tx.clone();
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

        // Start network sync if available
        if self.z_session.is_some() {
            self.sync_network();
        }

        // Declare liveliness if available
        if let (Some(session), Some(liveliness)) =
            (&self.z_session, &self.liveliness_state)
        {
            liveliness
                .declare_liveliness(session, &self.legacy_metadata)
                .await;
            liveliness.update(session, &self.legacy_metadata).await;
        }

        Ok(())
    }

    /// Get object by ID with event triggering
    pub async fn get_object(
        &self,
        object_id: u64,
    ) -> Result<Option<ObjectEntry>, ShardError> {
        let key = object_id.to_string();
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
    pub async fn set_object(
        &self,
        object_id: u64,
        entry: ObjectEntry,
    ) -> Result<(), ShardError> {
        let key = object_id.to_string();
        let storage_value = serialize_object_entry(&entry)?;

        // Perform the storage operation and get info about whether it was new
        let old_storage_value =
            self.set_storage_value(key, storage_value).await?;
        let is_new = old_storage_value.is_none();

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
                is_new,
            )
            .await;
        }

        Ok(())
    }

    /// Delete object with automatic event triggering
    /// All delete operations automatically trigger DataDelete events
    pub async fn delete_object(
        &self,
        object_id: &u64,
    ) -> Result<(), ShardError> {
        let key = object_id.to_string();

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
    pub async fn count_objects(&self) -> Result<usize, ShardError> {
        let count = self.count_storage_values().await?;
        Ok(count as usize)
    }

    /// Internal storage operations that handle replication
    async fn get_storage_value(
        &self,
        key: &str,
    ) -> Result<Option<StorageValue>, ShardError> {
        // All replication types can read from local app storage
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let key_bytes = key.as_bytes();
        match self.app_storage.get(key_bytes).await {
            Ok(Some(data)) => {
                let entry: StorageValue = bincode::serde::decode_from_slice(
                    data.as_slice(),
                    bincode::config::standard(),
                )
                .map(|(entry, _)| entry)
                .map_err(|e| ShardError::SerializationError(e.to_string()))?;
                Ok(Some(entry))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    async fn set_storage_value(
        &self,
        key: String,
        entry: StorageValue,
    ) -> Result<Option<StorageValue>, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // First get the old value for event purposes
        let old_value = self.get_storage_value(&key).await?;

        let value_bytes =
            bincode::serde::encode_to_vec(&entry, bincode::config::standard())
                .map_err(|e| ShardError::SerializationError(e.to_string()))?;

        match &self.replication {
            repl => {
                // Route through replication layer
                let operation = Operation::Write(WriteOperation {
                    key: StorageValue::from(key),
                    value: StorageValue::from(value_bytes),
                    ttl: None,
                });

                let request = ShardRequest {
                    operation,
                    timestamp: SystemTime::now(),
                    source_node: self.metadata.id,
                    request_id: uuid::Uuid::new_v4().to_string(),
                };

                let response = repl
                    .replicate_write(request)
                    .await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                match response.status {
                    ResponseStatus::Applied => Ok(old_value),
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(reason))
                    }
                    _ => Err(ShardError::ReplicationError(
                        "Unknown response".to_string(),
                    )),
                }
            }
        }
    }

    async fn delete_storage_value(
        &self,
        key: &str,
    ) -> Result<Option<StorageValue>, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // First get the old value for event purposes
        let old_value = self.get_storage_value(key).await?;

        match &self.replication {
            repl => {
                let operation =
                    Operation::Delete(DeleteOperation { key: key.into() });

                let request = ShardRequest {
                    operation,
                    timestamp: SystemTime::now(),
                    source_node: self.metadata.id,
                    request_id: uuid::Uuid::new_v4().to_string(),
                };

                let response = repl
                    .replicate_write(request)
                    .await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                match response.status {
                    ResponseStatus::Applied => Ok(old_value),
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(reason))
                    }
                    _ => Err(ShardError::ReplicationError(
                        "Unknown response".to_string(),
                    )),
                }
            }
        }
    }

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

    fn sync_network(&self) {
        // Clone the components we need for the async task
        let metadata = self.metadata.clone();
        let readiness_rx = self.readiness_rx.clone();
        let network = self.network.clone();
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
                            "shard {}: Failed to declare liveliness subscriber: {}",
                            metadata.id,
                            e
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
                            if let Some(network) = &network {
                                let mut net = network.lock().await;
                                if receiver.borrow().to_owned() {
                                    if !net.is_running() {
                                        info!("Start network for shard {}", metadata.id);
                                        if let Err(e) = net.start().await {
                                            error!("Failed to start network: {}", e);
                                        }
                                    }
                                } else {
                                    if net.is_running() {
                                        net.stop().await;
                                    }
                                }
                            }
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
    async fn trigger_data_events(
        &self,
        object_id: u64,
        new_entry: &ObjectEntry,
        old_entry: Option<&ObjectEntry>,
        is_new: bool,
    ) {
        use crate::events::types::EventType;

        if let Some(event_manager) = &self.event_manager {
            // Only trigger events if the new entry has events configured
            if new_entry.event.is_some() {
                let meta = &self.metadata;

                // Compare fields and trigger appropriate events
                for (field_id, new_val) in &new_entry.value {
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

    async fn trigger_delete_events(
        &self,
        object_id: u64,
        deleted_entry: &ObjectEntry,
    ) {
        use crate::events::types::EventType;

        if let Some(event_manager) = &self.event_manager {
            // Only trigger events if the deleted entry had events configured
            if deleted_entry.event.is_some() {
                let meta = &self.metadata;

                for field_id in deleted_entry.value.keys() {
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

    pub async fn close(self) -> Result<(), ShardError> {
        info!("{:?}: closing", self.metadata);
        self.token.cancel();

        if let Some(network) = &self.network {
            let mut net = network.lock().await;
            if net.is_running() {
                net.stop().await;
            }
        }

        if let Some(inv_manager) = &self.inv_net_manager {
            inv_manager.lock().await.stop().await;
        }

        Ok(())
    }

    /// Trigger an event if event manager is available
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

/// Transaction wrapper for ObjectUnifiedShard
pub struct ObjectShardTransaction<T: StorageTransaction> {
    storage_tx: Option<T>,
    metrics: Arc<ShardMetrics>,
    completed: bool,
}

#[async_trait::async_trait]
impl<T: StorageTransaction + 'static> ShardTransaction
    for ObjectShardTransaction<T>
{
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match &self.storage_tx {
            Some(tx) => {
                let key_str = key.to_string();
                let key_bytes = key_str.as_bytes();
                match tx.get(key_bytes).await {
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
        key: Self::Key,
        entry: Self::Entry,
    ) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match &mut self.storage_tx {
            Some(tx) => {
                let key_str = key.to_string();
                let key_bytes = key_str.as_bytes();
                let storage_value = serialize_object_entry(&entry)?;
                match tx.put(key_bytes, storage_value).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(ShardError::StorageError(e)),
                }
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn delete(&mut self, key: &Self::Key) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match &mut self.storage_tx {
            Some(tx) => {
                let key_str = key.to_string();
                let key_bytes = key_str.as_bytes();
                match tx.delete(key_bytes).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(ShardError::StorageError(e)),
                }
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match self.storage_tx.take() {
            Some(tx) => match tx.commit().await {
                Ok(_) => {
                    self.completed = true;
                    Ok(())
                }
                Err(e) => {
                    self.metrics
                        .errors_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Err(ShardError::StorageError(e))
                }
            },
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match self.storage_tx.take() {
            Some(tx) => match tx.rollback().await {
                Ok(_) => {
                    self.completed = true;
                    Ok(())
                }
                Err(e) => Err(ShardError::StorageError(e)),
            },
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
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
impl<A, R> UnifiedObjectShard for ObjectUnifiedShard<A, R>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + Default + 'static,
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

    async fn scan_objects(
        &self,
        prefix: Option<&u64>,
    ) -> Result<Vec<(u64, ObjectEntry)>, ShardError> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let prefix_str = prefix.map(|p| p.to_string()).unwrap_or_default();
        let prefix_bytes = prefix_str.as_bytes();
        match self.app_storage.scan(prefix_bytes).await {
            Ok(results) => {
                let mut converted = Vec::new();
                for (key_val, value_val) in results {
                    let key_str =
                        String::from_utf8_lossy(key_val.as_slice()).to_string();
                    if let Ok(key) = key_str.parse::<u64>() {
                        let storage_value: StorageValue =
                            bincode::serde::decode_from_slice(
                                value_val.as_slice(),
                                bincode::config::standard(),
                            )
                            .map(|(entry, _)| entry)
                            .map_err(|e| {
                                ShardError::SerializationError(e.to_string())
                            })?;

                        let entry = deserialize_object_entry(&storage_value)?;
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
    ) -> Result<Box<dyn UnifiedShardTransaction>, ShardError> {
        match self.app_storage.begin_transaction().await {
            Ok(storage_tx) => {
                let tx = ObjectShardTransaction {
                    storage_tx: Some(storage_tx),
                    metrics: self.metrics.clone(),
                    completed: false,
                };
                // Wrap the concrete transaction in a trait object adapter
                Ok(Box::new(UnifiedShardTransactionAdapter {
                    inner: Box::new(tx),
                }))
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
}

// Implement the IntoUnifiedShard trait for easy conversion
impl<A, R> IntoUnifiedShard for ObjectUnifiedShard<A, R>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + Default + 'static,
{
    fn into_boxed(self) -> BoxedUnifiedObjectShard {
        Box::new(self)
    }

    fn into_arc(self: Arc<Self>) -> ArcUnifiedObjectShard {
        self
    }
}

// Transaction adapter to bridge concrete transactions to the trait
struct UnifiedShardTransactionAdapter {
    inner: Box<
        dyn ShardTransaction<
            Key = u64,
            Entry = ObjectEntry,
            Error = ShardError,
        >,
    >,
}

#[async_trait::async_trait]
impl UnifiedShardTransaction for UnifiedShardTransactionAdapter {
    async fn get(&self, key: &u64) -> Result<Option<ObjectEntry>, ShardError> {
        self.inner.get(key).await
    }

    async fn set(
        &mut self,
        key: u64,
        entry: ObjectEntry,
    ) -> Result<(), ShardError> {
        self.inner.set(key, entry).await
    }

    async fn delete(&mut self, key: &u64) -> Result<(), ShardError> {
        self.inner.delete(key).await
    }

    async fn commit(self: Box<Self>) -> Result<(), ShardError> {
        self.inner.commit().await
    }

    async fn rollback(self: Box<Self>) -> Result<(), ShardError> {
        self.inner.rollback().await
    }
}
