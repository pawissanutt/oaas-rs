//! ObjectShard trait implementation for ObjectUnifiedShard

use std::collections::HashMap;
use std::sync::Arc;

use super::ObjectUnifiedShard;
use super::transaction::UnifiedShardWriteTxAdapter;
use crate::error::OdgmError;
use crate::events::{EventContext, EventManager};
use crate::granular_key::ObjectMetadata;
use crate::granular_trait::{
    EntryListOptions, EntryListResult, EntryStore, ObjectListOptions, ObjectListResult,
};
use crate::replication::ReplicationLayer;
use crate::shard::network::UnifiedShardNetwork;
use crate::shard::object_trait::{
    ArcUnifiedObjectShard, BoxedUnifiedObjectShard, IntoUnifiedShard, ObjectShard,
    UnifiedShardTransaction,
};
use crate::shard::{ObjectData, ObjectVal, ShardError, ShardMetadata};
use oprc_dp_storage::ApplicationDataStorage;
use oprc_grpc::{InvocationRequest, InvocationResponse, ObjectInvocationRequest};
use oprc_invoke::OffloadError;
use tokio::sync::{watch, Mutex};
use tracing::{info, instrument};

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
            if let Some((oid_str, crate::granular_key::GranularRecord::Metadata)) =
                crate::granular_key::parse_granular_key(k.as_slice())
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
    async fn batch_delete_objects(&self, keys: Vec<String>) -> Result<(), ShardError> {
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
        EntryStore::batch_set_entries(self, normalized_id, values, expected_version).await
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

    async fn ensure_metadata_exists(&self, normalized_id: &str) -> Result<bool, ShardError> {
        self.ensure_metadata_exists(normalized_id).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn UnifiedShardTransaction + '_>, ShardError> {
        match self.app_storage.begin_write_transaction() {
            Ok(storage_tx) => {
                let tx = UnifiedShardWriteTxAdapter::new(storage_tx, self.metrics.clone());
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

    async fn trigger_event_with_entry(&self, context: EventContext, object_entry: &ObjectData) {
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

    async fn list_objects(&self, options: ObjectListOptions) -> Result<ObjectListResult, ShardError> {
        EntryStore::list_objects(self, options).await
    }

    async fn start_network(&self, arc_self: ArcUnifiedObjectShard) -> Result<(), ShardError> {
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
        if let (Some(session), Some(liveliness)) = (&self.z_session, &self.liveliness_state) {
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
