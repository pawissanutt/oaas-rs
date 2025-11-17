use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;

use super::{config::ShardError, traits::ShardMetadata};
use crate::events::EventContext;
use crate::granular_key::ObjectMetadata;
use crate::granular_trait::{EntryListOptions, EntryListResult};
use crate::shard::{ObjectData, ObjectVal};
use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
};
use oprc_invoke::OffloadError;

/// Trait for unified object shards that provides a common interface regardless of storage/replication type
#[async_trait::async_trait]
pub trait ObjectShard: Send + Sync {
    /// Downcast helper (override in concrete implementations)
    fn as_any(&self) -> &dyn std::any::Any {
        panic!("as_any not implemented")
    }
    /// Get shard metadata
    fn meta(&self) -> &ShardMetadata;

    /// Watch shard readiness status
    fn watch_readiness(&self) -> watch::Receiver<bool>;

    /// Initialize the shard
    async fn initialize(&self) -> Result<(), ShardError>;

    /// Close the shard gracefully
    async fn close(&self) -> Result<(), ShardError>;

    /// Get object by ID
    async fn get_object(
        &self,
        object_id: u64,
    ) -> Result<Option<ObjectData>, ShardError>;

    /// Get object by normalized string ID (default unsupported).
    async fn get_object_by_str_id(
        &self,
        _normalized_id: &str,
    ) -> Result<Option<ObjectData>, ShardError> {
        Err(ShardError::InvalidKey)
    }

    /// Set object by ID with automatic event triggering
    /// All set operations automatically trigger appropriate events (DataCreate/DataUpdate)
    async fn set_object(
        &self,
        object_id: u64,
        entry: ObjectData,
    ) -> Result<(), ShardError>;

    /// Set object by normalized string ID (default unsupported).
    async fn set_object_by_str_id(
        &self,
        _normalized_id: &str,
        _entry: ObjectData,
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
    ) -> Result<Vec<(u64, ObjectData)>, ShardError>;

    /// Batch set multiple objects
    async fn batch_set_objects(
        &self,
        entries: Vec<(u64, ObjectData)>,
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
        object_entry: &ObjectData,
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

    /// Granular storage: get object metadata (version, flags).
    async fn get_metadata_granular(
        &self,
        _normalized_id: &str,
    ) -> Result<Option<ObjectMetadata>, ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: set object metadata explicitly (used for metadata-only object creation).
    async fn set_metadata_granular(
        &self,
        _normalized_id: &str,
        _metadata: ObjectMetadata,
    ) -> Result<(), ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Create metadata if absent, returns true if created, false if already existed.
    async fn ensure_metadata_exists(
        &self,
        _normalized_id: &str,
    ) -> Result<bool, ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: get a single entry value.
    async fn get_entry_granular(
        &self,
        _normalized_id: &str,
        _key: &str,
    ) -> Result<Option<ObjectVal>, ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: set a single entry value.
    async fn set_entry_granular(
        &self,
        _normalized_id: &str,
        _key: &str,
        _value: ObjectVal,
    ) -> Result<(), ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: delete a single entry.
    async fn delete_entry_granular(
        &self,
        _normalized_id: &str,
        _key: &str,
    ) -> Result<(), ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: list entries with pagination.
    async fn list_entries_granular(
        &self,
        _normalized_id: &str,
        _options: EntryListOptions,
    ) -> Result<EntryListResult, ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: batch set entries atomically.
    async fn batch_set_entries_granular(
        &self,
        _normalized_id: &str,
        _values: HashMap<String, ObjectVal>,
        _expected_version: Option<u64>,
    ) -> Result<u64, ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: batch delete entries atomically.
    async fn batch_delete_entries_granular(
        &self,
        _normalized_id: &str,
        _keys: Vec<String>,
    ) -> Result<(), ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Granular storage: reconstruct full object from entries.
    async fn reconstruct_object_granular(
        &self,
        _normalized_id: &str,
        _prefetch_limit: usize,
    ) -> Result<Option<ObjectData>, ShardError> {
        Err(ShardError::ConfigurationError(
            "granular storage not supported by this shard".into(),
        ))
    }

    /// Attach shard Arc and start network services (Zenoh subscribers/queryables).
    /// Implementations that support networking should override this to wire the network layer
    /// AFTER the shard is wrapped in an Arc so object_api calls can use trait object safely.
    async fn start_network(
        &self,
        _arc_self: ArcUnifiedObjectShard,
    ) -> Result<(), ShardError> {
        Err(ShardError::ConfigurationError(
            "network start unsupported by this shard".into(),
        ))
    }
}

/// Trait for unified shard transactions
#[async_trait::async_trait(?Send)]
pub trait UnifiedShardTransaction {
    /// Get object within transaction
    async fn get(&self, key: &u64) -> Result<Option<ObjectData>, ShardError>;

    /// Set object within transaction
    async fn set(
        &mut self,
        key: u64,
        entry: ObjectData,
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
