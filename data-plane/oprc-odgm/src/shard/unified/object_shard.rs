use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::config::ShardError;
use super::core::UnifiedShard;
use super::traits::{ReplicationType, ShardMetadata, ShardState, StorageType, ShardTransaction};
use crate::error::OdgmError;
use crate::events::{EventContext, EventManager};
use crate::replication::ReplicationLayer;
use crate::shard::{
    invocation::{InvocationNetworkManager, InvocationOffloader},
    liveliness::MemberLivelinessState,
    network::ShardNetwork,
    ObjectEntry, ShardState as LegacyShardState,
};
use oprc_dp_storage::{StorageBackend, StorageValue};

/// Unified ObjectShard that combines storage, networking, events, and management
/// This replaces both the old ObjectUnifiedShard and EnhancedObjectShard
pub struct ObjectUnifiedShard<S: StorageBackend, R: ReplicationLayer> {
    // Core storage functionality
    core: Arc<UnifiedShard<S, R>>,
    
    // Networking components (optional - can be disabled for storage-only use)
    z_session: Option<zenoh::Session>,
    inv_net_manager: Option<Arc<Mutex<InvocationNetworkManager>>>,
    inv_offloader: Option<Arc<InvocationOffloader>>,
    network: Option<Arc<Mutex<ShardNetwork>>>,
    
    // Event management (optional)
    event_manager: Option<Arc<EventManager>>,
    
    // Liveliness management (optional)
    liveliness_state: Option<MemberLivelinessState>,
    
    // Control and metadata
    token: CancellationToken,
    legacy_metadata: crate::shard::ShardMetadata,
}

impl<S: StorageBackend + 'static, R: ReplicationLayer + Default + 'static> ObjectUnifiedShard<S, R> {
    /// Create a new ObjectUnifiedShard with full networking and event support
    pub async fn new_full(
        metadata: ShardMetadata,
        storage: S,
        replication: Option<R>,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<Self, ShardError> {
        let core = Arc::new(UnifiedShard::new(metadata.clone(), storage, replication).await?);
        let legacy_metadata = metadata.clone().into();
        
        // Set up networking components
        let inv_offloader = Arc::new(InvocationOffloader::new(&legacy_metadata));
        let inv_net_manager = InvocationNetworkManager::new(
            z_session.clone(),
            legacy_metadata.clone(),
            inv_offloader.clone(),
        );
        
        // Create a basic shard for network compatibility (temporary)
        use crate::shard::BasicObjectShard;
        let basic_shard = Arc::new(BasicObjectShard::new(legacy_metadata.clone())) 
            as Arc<dyn LegacyShardState<Key = u64, Entry = ObjectEntry>>;
        let prefix = format!(
            "oprc/{}/{}/objects",
            metadata.collection, 
            metadata.partition_id
        );
        let network = ShardNetwork::new(z_session.clone(), basic_shard, prefix);
        
        Ok(Self {
            core,
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
    
    /// Create a storage-only ObjectUnifiedShard (no networking)
    pub async fn new(
        metadata: ShardMetadata,
        storage: S,
        replication: Option<R>,
    ) -> Result<Self, ShardError> {
        let core = Arc::new(UnifiedShard::new(metadata.clone(), storage, replication).await?);
        let legacy_metadata = metadata.into();
        
        Ok(Self {
            core,
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
        // Initialize core storage
        self.core.initialize().await?;
        
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
        if let (Some(session), Some(liveliness)) = (&self.z_session, &self.liveliness_state) {
            liveliness.declare_liveliness(session, &self.legacy_metadata).await;
            liveliness.update(session, &self.legacy_metadata).await;
        }
        
        Ok(())
    }

    /// Get object by ID with event triggering
    pub async fn get_object(&self, object_id: u64) -> Result<Option<ObjectEntry>, ShardError> {
        let key = object_id.to_string();
        match self.core.get(&key).await? {
            Some(storage_value) => {
                let entry = deserialize_object_entry(&storage_value)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// Set object with event triggering
    pub async fn set_object(&self, object_id: u64, entry: ObjectEntry) -> Result<(), ShardError> {
        let key = object_id.to_string();
        let storage_value = serialize_object_entry(&entry)?;
        
        // Check if this is a new object for event purposes
        let is_new = self.core.get(&key).await?.is_none();
        let old_entry = if !is_new && self.event_manager.is_some() {
            self.get_object(object_id).await?
        } else {
            None
        };
        
        // Perform the storage operation
        self.core.set(key, storage_value).await?;
        
        // Trigger events if event manager is available
        if self.event_manager.is_some() {
            self.trigger_data_events(object_id, &entry, old_entry.as_ref(), is_new).await;
        }
        
        Ok(())
    }

    /// Delete object with event triggering  
    pub async fn delete_object(&self, object_id: &u64) -> Result<(), ShardError> {
        let key = object_id.to_string();
        
        // Get the entry before deletion for event purposes
        let deleted_entry = if self.event_manager.is_some() {
            self.get_object(*object_id).await?
        } else {
            None
        };
        
        // Perform the deletion
        self.core.delete(&key).await?;
        
        // Trigger delete events if available
        if let (Some(_), Some(entry)) = (&self.event_manager, deleted_entry) {
            self.trigger_delete_events(*object_id, &entry).await;
        }
        
        Ok(())
    }

    /// Get count of objects
    pub async fn count_objects(&self) -> Result<usize, ShardError> {
        let count = self.core.count().await?;
        Ok(count as usize)
    }

    fn sync_network(&self) {
        let shard = self.clone();
        if let Some(session) = &self.z_session {
            let session = session.clone();
            tokio::spawn(async move {
                let mut receiver = shard.core.watch_readiness();
                let liveliness_selector = format!(
                    "oprc/{}/{}/liveliness/*",
                    shard.core.meta().collection,
                    shard.core.meta().partition_id
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
                            shard.core.meta().id,
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
                            if let Some(network) = &shard.network {
                                let mut net = network.lock().await;
                                if receiver.borrow().to_owned() {
                                    if !net.is_running() {
                                        info!("Start network for shard {}", shard.core.meta().id);
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
                                    if let Some(liveliness) = &shard.liveliness_state {
                                        let id = liveliness.handle_sample(&sample).await;
                                        info!("shard {}: liveliness {id:?} updated {sample:?}", shard.core.meta().id );
                                        if id != Some(shard.core.meta().id) {
                                            if let Some(inv_manager) = &shard.inv_net_manager {
                                                let mut inv = inv_manager.lock().await;
                                                inv.on_liveliness_updated(liveliness).await;
                                            }
                                        }
                                    }
                                },
                                Err(_) => {},
                            };
                        }
                        _ = shard.token.cancelled() => {
                            break;
                        }
                    }
                }
                info!(
                    "shard {}: Network sync loop exited",
                    shard.core.meta().id
                );
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
                let meta = self.core.meta();

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
                    event_manager.trigger_event_with_entry(context, new_entry).await;
                }
            }
        }
    }

    async fn trigger_delete_events(&self, object_id: u64, deleted_entry: &ObjectEntry) {
        use crate::events::types::EventType;

        if let Some(event_manager) = &self.event_manager {
            // Only trigger events if the deleted entry had events configured
            if deleted_entry.event.is_some() {
                let meta = self.core.meta();

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
                    event_manager.trigger_event_with_entry(context, deleted_entry).await;
                }
            }
        }
    }

    pub async fn close(self) -> Result<(), ShardError> {
        info!("{:?}: closing", self.core.meta());
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
}

// Implement the unified ShardState trait
#[async_trait::async_trait]
impl<S: StorageBackend + 'static, R: ReplicationLayer + Default + 'static> ShardState for ObjectUnifiedShard<S, R> {
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;

    fn meta(&self) -> &ShardMetadata {
        self.core.meta()
    }

    fn storage_type(&self) -> StorageType {
        self.core.storage_type()
    }

    fn replication_type(&self) -> ReplicationType {
        self.core.replication_type()
    }

    async fn initialize(&self) -> Result<(), Self::Error> {
        self.core.initialize().await
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        // Note: This consumes self, so we'll implement a separate close method
        Ok(())
    }

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Entry>, Self::Error> {
        self.get_object(*key).await
    }

    async fn set(&self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error> {
        self.set_object(key, entry).await
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), Self::Error> {
        self.delete_object(key).await
    }

    async fn count(&self) -> Result<u64, Self::Error> {
        Ok(self.count_objects().await? as u64)
    }

    async fn scan(
        &self,
        prefix: Option<&Self::Key>,
    ) -> Result<Vec<(Self::Key, Self::Entry)>, Self::Error> {
        let prefix_str = prefix.map(|k| k.to_string());
        let results = self.core.scan(prefix_str.as_ref()).await?;

        let mut object_results = Vec::new();
        for (key_str, storage_value) in results {
            if let Ok(key) = key_str.parse::<u64>() {
                if let Ok(entry) = deserialize_object_entry(&storage_value) {
                    object_results.push((key, entry));
                }
            }
        }
        Ok(object_results)
    }

    async fn batch_set(&self, entries: Vec<(Self::Key, Self::Entry)>) -> Result<(), Self::Error> {
        let mut storage_entries = Vec::new();
        for (key, entry) in entries {
            let key_str = key.to_string();
            let storage_value = serialize_object_entry(&entry)?;
            storage_entries.push((key_str, storage_value));
        }
        self.core.batch_set(storage_entries).await
    }

    async fn batch_delete(&self, keys: Vec<Self::Key>) -> Result<(), Self::Error> {
        let key_strs: Vec<String> = keys.into_iter().map(|k| k.to_string()).collect();
        self.core.batch_delete(key_strs).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<
        Box<dyn ShardTransaction<Key = Self::Key, Entry = Self::Entry, Error = Self::Error>>,
        Self::Error,
    > {
        let inner_tx = self.core.begin_transaction().await?;
        Ok(Box::new(ObjectShardTransaction::<S, R> { 
            inner_tx, 
            _phantom: std::marker::PhantomData 
        }))
    }

    fn watch_readiness(&self) -> watch::Receiver<bool> {
        self.core.watch_readiness()
    }
}

// Implement the legacy ShardState trait for compatibility
#[async_trait::async_trait]
impl<S: StorageBackend + 'static, R: ReplicationLayer + Default + 'static> LegacyShardState for ObjectUnifiedShard<S, R> {
    type Key = u64;
    type Entry = ObjectEntry;

    fn meta(&self) -> &crate::shard::ShardMetadata {
        &self.legacy_metadata
    }

    fn watch_readiness(&self) -> watch::Receiver<bool> {
        self.core.watch_readiness()
    }

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Entry>, OdgmError> {
        match self.get_object(*key).await {
            Ok(result) => Ok(result),
            Err(e) => Err(OdgmError::from(e)),
        }
    }

    async fn set(&self, key: Self::Key, value: Self::Entry) -> Result<(), OdgmError> {
        match self.set_object(key, value).await {
            Ok(()) => Ok(()),
            Err(e) => Err(OdgmError::from(e)),
        }
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError> {
        match self.delete_object(key).await {
            Ok(()) => Ok(()),
            Err(e) => Err(OdgmError::from(e)),
        }
    }

    async fn count(&self) -> Result<u64, OdgmError> {
        match self.count_objects().await {
            Ok(count) => Ok(count as u64),
            Err(e) => Err(OdgmError::from(e)),
        }
    }
}

impl<S: StorageBackend, R: ReplicationLayer> Clone for ObjectUnifiedShard<S, R> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            z_session: self.z_session.clone(),
            inv_net_manager: self.inv_net_manager.clone(),
            inv_offloader: self.inv_offloader.clone(),
            network: self.network.clone(),
            event_manager: self.event_manager.clone(),
            liveliness_state: self.liveliness_state.clone(),
            token: self.token.clone(),
            legacy_metadata: self.legacy_metadata.clone(),
        }
    }
}

/// Transaction wrapper for ObjectUnifiedShard
pub struct ObjectShardTransaction<S: StorageBackend, R: ReplicationLayer> {
    inner_tx: Box<dyn super::traits::ShardTransaction<Key = String, Entry = StorageValue, Error = ShardError>>,
    _phantom: std::marker::PhantomData<(S, R)>,
}

#[async_trait::async_trait]
impl<S: StorageBackend, R: ReplicationLayer> ShardTransaction for ObjectShardTransaction<S, R> {
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Entry>, Self::Error> {
        let key_str = key.to_string();
        match self.inner_tx.get(&key_str).await? {
            Some(storage_value) => {
                let entry = deserialize_object_entry(&storage_value)?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    async fn set(&mut self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error> {
        let key_str = key.to_string();
        let storage_value = serialize_object_entry(&entry)?;
        self.inner_tx.set(key_str, storage_value).await
    }

    async fn delete(&mut self, key: &Self::Key) -> Result<(), Self::Error> {
        let key_str = key.to_string();
        self.inner_tx.delete(&key_str).await
    }

    async fn commit(self: Box<Self>) -> Result<(), Self::Error> {
        self.inner_tx.commit().await
    }

    async fn rollback(self: Box<Self>) -> Result<(), Self::Error> {
        self.inner_tx.rollback().await
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
