//! Network layer for unified shard
//!
//! This module is split into:
//! - `mod.rs` - UnifiedShardNetwork struct and main impl
//! - `helpers.rs` - parsing helper functions
//! - `setter.rs` - UnifiedSetterHandler for SET operations
//! - `getter.rs` - UnifiedGetterHandler for GET operations  
//! - `batch_setter.rs` - UnifiedBatchSetHandler for batch SET operations
//! - `list_objects.rs` - ListObjectsHandler for list operations

mod batch_setter;
mod getter;
mod helpers;
mod list_objects;
mod setter;

use std::sync::Arc;

use flume::Receiver;
use tokio_util::sync::CancellationToken;
use zenoh::{
    pubsub::Subscriber,
    query::{Query, Queryable},
    sample::Sample,
};

use crate::replication::ReplicationLayer;
use crate::shard::object_trait::ObjectShard;
use crate::shard::ShardMetadata;
use oprc_zenoh::util::{ManagedConfig, declare_managed_queryable, declare_managed_subscriber};

use batch_setter::UnifiedBatchSetHandler;
use getter::UnifiedGetterHandler;
use list_objects::ListObjectsHandler;
use setter::UnifiedSetterHandler;

/// Modern unified shard network interface
/// Works with ReplicationLayer instead of direct shard access for better consistency
pub struct UnifiedShardNetwork<R: ReplicationLayer> {
    z_session: zenoh::Session,
    replication: Arc<R>,
    metadata: ShardMetadata,
    token: CancellationToken,
    prefix: String,
    max_string_id_len: usize,
    set_subscriber: Option<Subscriber<Receiver<Sample>>>,
    set_queryable: Option<Queryable<Receiver<Query>>>,
    get_queryable: Option<Queryable<Receiver<Query>>>,
    batch_set_queryable: Option<Queryable<Receiver<Query>>>,
    list_objects_queryable: Option<Queryable<Receiver<Query>>>,
    // New: reference to shard for granular reconstruction & mutations
    shard: Option<Arc<dyn ObjectShard>>, // attached after shard creation
}

impl<R: ReplicationLayer + 'static> UnifiedShardNetwork<R> {
    pub fn new(
        z_session: zenoh::Session,
        replication: Arc<R>,
        metadata: ShardMetadata,
        prefix: String,
        max_string_id_len: usize,
    ) -> Self {
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            replication,
            metadata,
            token,
            prefix,
            max_string_id_len,
            set_subscriber: None,
            set_queryable: None,
            get_queryable: None,
            batch_set_queryable: None,
            list_objects_queryable: None,
            shard: None,
        }
    }

    /// Attach the concrete shard after it has been constructed and wrapped in Arc.
    /// This enables GET/SET handlers to use unified granular APIs.
    pub fn attach_shard(&mut self, shard: Arc<dyn ObjectShard>) {
        self.shard = Some(shard);
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Starting network for {} with prefix '{}'",
            self.metadata.id,
            self.prefix
        );
        self.token = CancellationToken::new();
        self.start_set_subscriber().await?;
        self.start_set_queryable().await?;
        self.start_get_queryable().await?;
        self.start_batch_set_queryable().await?;
        self.start_list_objects_queryable().await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(id=%self.metadata.id))]
    pub async fn stop(&mut self) {
        if let Some(sub) = self.set_subscriber.take() {
            if let Err(e) = sub.undeclare().await {
                tracing::warn!("Failed to undeclare subscriber: {}", e);
            };
        }
        if let Some(q) = self.set_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
        if let Some(q) = self.get_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
        if let Some(q) = self.batch_set_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
        if let Some(q) = self.list_objects_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare list_objects queryable: {}", e);
            };
        }
    }

    pub fn is_running(&self) -> bool {
        !self.token.is_cancelled()
    }

    async fn start_set_subscriber(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*", self.prefix);
        let handler = UnifiedSetterHandler::new(
            self.replication.clone(),
            self.metadata.clone(),
            self.max_string_id_len,
            self.shard.clone(),
        );
        let config = ManagedConfig::new(key, 16, 65536);
        let s = declare_managed_subscriber(&self.z_session, config, handler).await?;
        self.set_subscriber = Some(s);
        Ok(())
    }

    async fn start_set_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*/set", self.prefix);
        let handler = UnifiedSetterHandler::new(
            self.replication.clone(),
            self.metadata.clone(),
            self.max_string_id_len,
            self.shard.clone(),
        );
        let config = ManagedConfig::new(key, 16, 65536);
        let q = declare_managed_queryable(&self.z_session, config, handler).await?;
        self.set_queryable = Some(q);
        Ok(())
    }

    async fn start_get_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/**", self.prefix);
        let handler = UnifiedGetterHandler::new(
            self.replication.clone(),
            self.metadata.clone(),
            self.max_string_id_len,
            self.shard.clone(),
        );
        let config = ManagedConfig::new(key, 16, 65536);
        let q = declare_managed_queryable(&self.z_session, config, handler).await?;
        self.get_queryable = Some(q);
        Ok(())
    }

    async fn start_batch_set_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*/batch-set", self.prefix);
        let handler = UnifiedBatchSetHandler::new(
            self.replication.clone(),
            self.metadata.clone(),
            self.max_string_id_len,
        );
        let config = ManagedConfig::new(key, 16, 65536);
        let q = declare_managed_queryable(&self.z_session, config, handler).await?;
        self.batch_set_queryable = Some(q);
        Ok(())
    }

    async fn start_list_objects_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Key pattern: oprc/<cls>/<pid>/list-objects
        let key = format!("{}/list-objects", self.prefix);
        let handler = ListObjectsHandler::new(self.shard.clone(), self.metadata.clone());
        let config = ManagedConfig::new(key, 16, 65536);
        let q = declare_managed_queryable(&self.z_session, config, handler).await?;
        self.list_objects_queryable = Some(q);
        Ok(())
    }
}
