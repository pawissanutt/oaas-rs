use std::sync::Arc;
use tokio::sync::watch;

use super::{config::ShardError, traits::ShardMetadata};
use crate::events::EventContext;
use crate::shard::ObjectEntry;
use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
};
use oprc_invoke::OffloadError;

/// Trait for unified object shards that provides a common interface regardless of storage/replication type
#[async_trait::async_trait]
pub trait ObjectShard: Send + Sync {
    /// Get shard metadata
    fn meta(&self) -> &ShardMetadata;

    /// Watch shard readiness status
    fn watch_readiness(&self) -> watch::Receiver<bool>;

    /// Initialize the shard
    async fn initialize(&self) -> Result<(), ShardError>;

    /// Close the shard gracefully
    async fn close(self: Box<Self>) -> Result<(), ShardError>;

    /// Get object by ID
    async fn get_object(
        &self,
        object_id: u64,
    ) -> Result<Option<ObjectEntry>, ShardError>;

    /// Get object by normalized string ID (default unsupported).
    async fn get_object_by_str_id(
        &self,
        _normalized_id: &str,
    ) -> Result<Option<ObjectEntry>, ShardError> {
        Err(ShardError::InvalidKey)
    }

    /// Set object by ID with automatic event triggering
    /// All set operations automatically trigger appropriate events (DataCreate/DataUpdate)
    async fn set_object(
        &self,
        object_id: u64,
        entry: ObjectEntry,
    ) -> Result<(), ShardError>;

    /// Set object by normalized string ID (default unsupported).
    async fn set_object_by_str_id(
        &self,
        _normalized_id: &str,
        _entry: ObjectEntry,
    ) -> Result<(), ShardError> {
        Err(ShardError::InvalidKey)
    }

    /// Delete object by ID with automatic event triggering
    /// All delete operations automatically trigger DataDelete events
    async fn delete_object(&self, object_id: &u64) -> Result<(), ShardError>;

    /// Delete object by normalized string ID (default unsupported).
    async fn delete_object_by_str_id(
        &self,
        _normalized_id: &str,
    ) -> Result<(), ShardError> {
        Err(ShardError::InvalidKey)
    }

    /// Count total objects in the shard
    async fn count_objects(&self) -> Result<usize, ShardError>;

    /// Scan objects with optional prefix
    async fn scan_objects(
        &self,
        prefix: Option<&u64>,
    ) -> Result<Vec<(u64, ObjectEntry)>, ShardError>;

    /// Batch set multiple objects
    async fn batch_set_objects(
        &self,
        entries: Vec<(u64, ObjectEntry)>,
    ) -> Result<(), ShardError>;

    /// Batch delete multiple objects
    async fn batch_delete_objects(
        &self,
        keys: Vec<u64>,
    ) -> Result<(), ShardError>;

    /// Begin a transaction
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn UnifiedShardTransaction + '_>, ShardError>;

    /// Trigger an event if event manager is available
    async fn trigger_event(&self, context: EventContext);

    /// Trigger event with object entry already available (more efficient)
    async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectEntry,
    );

    /// Invoke a function with the given request
    async fn invoke_fn(
        &self,
        req: InvocationRequest,
    ) -> Result<InvocationResponse, OffloadError>;

    /// Invoke an object method with the given request
    async fn invoke_obj(
        &self,
        req: ObjectInvocationRequest,
    ) -> Result<InvocationResponse, OffloadError>;
}

/// Trait for unified shard transactions
#[async_trait::async_trait(?Send)]
pub trait UnifiedShardTransaction {
    /// Get object within transaction
    async fn get(&self, key: &u64) -> Result<Option<ObjectEntry>, ShardError>;

    /// Set object within transaction
    async fn set(
        &mut self,
        key: u64,
        entry: ObjectEntry,
    ) -> Result<(), ShardError>;

    /// Delete object within transaction
    async fn delete(&mut self, key: &u64) -> Result<(), ShardError>;

    /// Commit the transaction
    async fn commit(self: Box<Self>) -> Result<(), ShardError>;

    /// Rollback the transaction
    async fn rollback(self: Box<Self>) -> Result<(), ShardError>;
}

/// Type alias for a boxed unified object shard
pub type BoxedUnifiedObjectShard = Box<dyn ObjectShard>;

/// Type alias for an Arc-wrapped unified object shard
pub type ArcUnifiedObjectShard = Arc<dyn ObjectShard>;

/// Helper trait for converting concrete shards to trait objects
pub trait IntoUnifiedShard {
    fn into_boxed(self) -> BoxedUnifiedObjectShard;
    fn into_arc(self) -> ArcUnifiedObjectShard;
}
