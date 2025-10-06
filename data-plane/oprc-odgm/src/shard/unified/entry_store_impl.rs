//! EntryStore trait implementation for ObjectUnifiedShard.
//! Phase C: Implement granular per-entry operations at the shard layer.
//!
//! Architecture:
//! - Shard layer: Encodes composite keys, manages versions, coordinates with replication
//! - Replication layer: Handles consensus on opaque binary key-value pairs
//! - Storage backend: Provides raw KV primitives (get/put/scan/delete)

use async_trait::async_trait;
use oprc_dp_storage::{ApplicationDataStorage, StorageValue};
use std::collections::HashMap;
use tracing::{debug, instrument, warn};

const MAX_LIST_LIMIT: usize = 1024;
const LIST_SCAN_SLACK: usize = 16;

use crate::events::EventManager;
use crate::events::{
    ChangedKey, MutAction, MutationContext, build_bridge_event,
};
use crate::granular_key::{
    GranularRecord, ObjectMetadata, build_entry_key, build_metadata_key,
    build_object_prefix, parse_granular_key,
};
use crate::granular_trait::{EntryListOptions, EntryListResult, EntryStore};
use crate::replication::{
    DeleteOperation, Operation, ReplicationLayer, ShardRequest, WriteOperation,
};
use crate::shard::ObjectVal;
use crate::shard::unified::config::ShardError;
use crate::shard::unified::object_shard::ObjectUnifiedShard;
use crate::storage_key::string_object_event_config_key;
use oprc_grpc::ObjectEvent;
use prost::Message as _;

#[async_trait]
impl<A, R, E> EntryStore for ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
    E: EventManager + Send + Sync + 'static,
{
    #[instrument(skip(self), fields(obj_id = normalized_id))]
    async fn get_metadata(
        &self,
        normalized_id: &str,
    ) -> Result<Option<ObjectMetadata>, ShardError> {
        let key = build_metadata_key(normalized_id);

        match self.app_storage.get(&key).await {
            Ok(Some(value)) => {
                match ObjectMetadata::from_bytes(value.as_slice()) {
                    Some(meta) => Ok(Some(meta)),
                    None => {
                        warn!(
                            "Failed to deserialize metadata for object {}",
                            normalized_id
                        );
                        Err(ShardError::InvalidMetadata)
                    }
                }
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Storage error reading metadata: {}", e);
                Err(ShardError::from(e))
            }
        }
    }

    #[instrument(skip(self, metadata), fields(obj_id = normalized_id))]
    async fn set_metadata(
        &self,
        normalized_id: &str,
        metadata: ObjectMetadata,
    ) -> Result<(), ShardError> {
        let key = build_metadata_key(normalized_id);
        let value = metadata.to_bytes();

        // Create WriteOperation and replicate through consensus
        let operation = Operation::Write(WriteOperation {
            key: StorageValue::from(key),
            value: StorageValue::from(value),
            ..Default::default()
        });
        let request = ShardRequest::from_operation(operation, 0); // TODO: Get proper node_id

        self.replication
            .replicate_write(request)
            .await
            .map_err(ShardError::from)?;

        Ok(())
    }

    #[instrument(skip(self), fields(obj_id = normalized_id, key = key))]
    async fn get_entry(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<Option<ObjectVal>, ShardError> {
        let storage_key = build_entry_key(normalized_id, key);

        match self.app_storage.get(&storage_key).await {
            Ok(Some(value)) => {
                // Deserialize ObjectVal from bytes using bincode 2.x API
                let decode_result: Result<(ObjectVal, _), _> =
                    bincode::serde::decode_from_slice(
                        value.as_slice(),
                        bincode::config::standard(),
                    );
                match decode_result {
                    Ok((val, _)) => {
                        // Emit metrics
                        self.metrics.inc_entry_reads();
                        Ok(Some(val))
                    }
                    Err(e) => {
                        warn!("Failed to deserialize entry value: {}", e);
                        Err(ShardError::DeserializationError)
                    }
                }
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Storage error reading entry: {}", e);
                Err(ShardError::from(e))
            }
        }
    }

    #[instrument(skip(self, value), fields(obj_id = normalized_id, key = key))]
    async fn set_entry(
        &self,
        normalized_id: &str,
        key: &str,
        value: ObjectVal,
    ) -> Result<(), ShardError> {
        // Encode raw ObjectVal (granular storage stores individual entry values only)
        let value_bytes =
            bincode::serde::encode_to_vec(&value, bincode::config::standard())
                .map_err(|e| ShardError::SerializationError(e.to_string()))?;

        let storage_key = build_entry_key(normalized_id, key);
        let v2_mode = std::env::var("ODGM_EVENT_PIPELINE_V2")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);
        let existed_before = if v2_mode {
            self.app_storage
                .exists(&storage_key)
                .await
                .map_err(ShardError::from)?
        } else {
            false
        };

        // Create WriteOperation and replicate
        let operation = Operation::Write(WriteOperation {
            key: StorageValue::from(storage_key),
            value: StorageValue::from(value_bytes),
            ..Default::default()
        });
        let request = ShardRequest::from_operation(operation, 0); // TODO: Get proper node_id

        self.replication
            .replicate_write(request)
            .await
            .map_err(ShardError::from)?;

        // Get or create metadata and increment version
        let mut metadata =
            self.get_metadata(normalized_id).await?.unwrap_or_default();
        let version_before = metadata.object_version;
        metadata.increment_version();
        let new_version_for_event = metadata.object_version; // capture before move
        self.set_metadata(normalized_id, metadata).await?;

        // Emit metrics
        self.metrics.inc_entry_writes();

        if v2_mode {
            let event_cfg = {
                let key_ev = string_object_event_config_key(normalized_id);
                match self.app_storage.get(&key_ev).await {
                    Ok(Some(val)) => ObjectEvent::decode(val.as_slice())
                        .ok()
                        .map(|ev| std::sync::Arc::new(ev)),
                    _ => None,
                }
            };
            let action = if existed_before {
                MutAction::Update
            } else {
                MutAction::Create
            };
            let ctx = MutationContext::new(
                normalized_id.to_string(),
                self.class_id().to_string(),
                self.partition_id_u16(),
                version_before,
                new_version_for_event,
                vec![ChangedKey {
                    key_canonical: key.to_string(),
                    action,
                }],
            )
            .with_event_config(event_cfg);
            if let Some(v2) = &self.v2_dispatcher {
                v2.try_send(ctx);
            }
        } else if let Some(bridge) = &self.bridge_dispatcher {
            let evt = build_bridge_event(
                self.class_id(),
                self.partition_id_u16(),
                normalized_id,
                new_version_for_event,
                vec![key.to_string()],
                bridge,
            );
            if bridge.try_send(evt) {
                self.metrics.inc_bridge_emitted();
            } else {
                self.metrics.inc_bridge_dropped();
            }
        }

        Ok(())
    }

    #[instrument(skip(self), fields(obj_id = normalized_id, key = key))]
    async fn delete_entry(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<(), ShardError> {
        let storage_key = build_entry_key(normalized_id, key);

        // Check if entry exists before deleting
        let exists = self
            .app_storage
            .exists(&storage_key)
            .await
            .map_err(ShardError::from)?;

        if !exists {
            // Idempotent - already deleted
            return Ok(());
        }

        // Create DeleteOperation and replicate
        let operation = Operation::Delete(DeleteOperation {
            key: StorageValue::from(storage_key),
        });
        let request = ShardRequest::from_operation(operation, 0); // TODO: Get proper node_id

        self.replication
            .replicate_write(request)
            .await
            .map_err(ShardError::from)?;

        // Increment version only if entry was actually deleted
        let mut metadata =
            self.get_metadata(normalized_id).await?.unwrap_or_default();
        let version_before = metadata.object_version;
        metadata.increment_version();
        let version_after = metadata.object_version;
        self.set_metadata(normalized_id, metadata).await?;

        // V2 delete mutation context
        let v2_mode = std::env::var("ODGM_EVENT_PIPELINE_V2")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);
        if v2_mode {
            let ctx = MutationContext::new(
                normalized_id.to_string(),
                self.class_id().to_string(),
                self.partition_id_u16(),
                version_before,
                version_after,
                vec![ChangedKey {
                    key_canonical: key.to_string(),
                    action: MutAction::Delete,
                }],
            );
            if let Some(v2) = &self.v2_dispatcher {
                v2.try_send(ctx);
            }
        }

        // Emit metrics
        self.metrics.inc_entry_deletes();

        Ok(())
    }

    /// Paginated, prefix-aware entry listing backed by storage-level range scans.
    ///
    /// The implementation pulls chunks via `scan_range_paginated`, skips metadata
    /// keys, and applies prefix filtering while accumulating results. A small
    /// slack is added to each storage read to tolerate filtered rows without
    /// issuing excessive range scans. The returned cursor is the raw storage key
    /// of the last entry emitted, allowing callers to resume without re-scanning
    /// previously observed records.
    #[instrument(
        skip(self, options),
        fields(
            obj_id = normalized_id,
            limit = options.limit,
            prefix = options.key_prefix.as_deref(),
            has_cursor = options.cursor.is_some()
        )
    )]
    async fn list_entries(
        &self,
        normalized_id: &str,
        options: EntryListOptions,
    ) -> Result<EntryListResult, ShardError> {
        if options.limit == 0 {
            return Ok(EntryListResult {
                entries: Vec::new(),
                next_cursor: options.cursor,
            });
        }

        let limit = options.limit.min(MAX_LIST_LIMIT);
        let object_prefix = build_object_prefix(normalized_id);
        let range_end = compute_object_range_end(&object_prefix);
        let config = bincode::config::standard();

        let mut scan_start = if let Some(cursor) = options.cursor.clone() {
            match parse_granular_key(cursor.as_slice()) {
                Some((ref obj_id, _)) if obj_id == normalized_id => cursor,
                _ => {
                    warn!(
                        "Ignoring pagination cursor that does not belong to object {}",
                        normalized_id
                    );
                    build_entry_key(normalized_id, "")
                }
            }
        } else {
            build_entry_key(normalized_id, "")
        };

        let mut skip_cursor_match = options.cursor.is_some();
        let key_prefix = options.key_prefix.as_deref();
        let mut entries = Vec::with_capacity(limit.min(32));
        let mut last_emitted_key: Option<Vec<u8>> = None;
        let mut more_available = false;
        let mut prefix_seen = false;
        let mut prefix_exhausted = false;

        'outer: while entries.len() < limit {
            let remaining = limit - entries.len();
            let chunk_limit = remaining + LIST_SCAN_SLACK;
            let (chunk, storage_cursor_raw) = self
                .app_storage
                .scan_range_paginated(
                    scan_start.as_slice(),
                    range_end.as_slice(),
                    Some(chunk_limit),
                )
                .await
                .map_err(ShardError::from)?;

            if chunk.is_empty() {
                break;
            }

            let mut storage_cursor =
                storage_cursor_raw.map(StorageValue::into_vec);
            let mut chunk_iter = chunk.into_iter().peekable();

            while let Some((raw_key, raw_value)) = chunk_iter.next() {
                if skip_cursor_match
                    && raw_key.as_slice() == scan_start.as_slice()
                {
                    skip_cursor_match = false;
                    continue;
                }
                skip_cursor_match = false;

                match parse_granular_key(raw_key.as_slice()) {
                    Some((ref obj_id, GranularRecord::Entry(entry_key)))
                        if obj_id == normalized_id =>
                    {
                        if let Some(prefix) = key_prefix {
                            if !entry_key.starts_with(prefix) {
                                if prefix_seen {
                                    prefix_exhausted = true;
                                    break 'outer;
                                }
                                continue;
                            }
                            prefix_seen = true;
                        }

                        match bincode::serde::decode_from_slice::<ObjectVal, _>(
                            raw_value.as_slice(),
                            config,
                        ) {
                            Ok((val, _)) => {
                                entries.push((entry_key.to_owned(), val));
                                last_emitted_key =
                                    Some(raw_key.as_slice().to_vec());

                                if entries.len() == limit {
                                    let chunk_has_more =
                                        chunk_iter.peek().is_some();
                                    let mut additional_matches = chunk_has_more
                                        || storage_cursor.is_some();

                                    if let Some(prefix) = key_prefix {
                                        additional_matches = if let Some((
                                            next_key,
                                            _,
                                        )) =
                                            chunk_iter.peek()
                                        {
                                            match parse_granular_key(next_key.as_slice()) {
                                                Some((ref next_obj, GranularRecord::Entry(next_entry)))
                                                    if next_obj == normalized_id
                                                        && next_entry.starts_with(prefix) =>
                                                {
                                                    true
                                                }
                                                _ => storage_cursor
                                                    .as_ref()
                                                    .map_or(false, |cursor_key| {
                                                        match parse_granular_key(
                                                            cursor_key.as_slice(),
                                                        ) {
                                                            Some((ref next_obj, GranularRecord::Entry(_)))
                                                                if next_obj
                                                                    == normalized_id => true,
                                                            _ => false,
                                                        }
                                                    }),
                                            }
                                        } else {
                                            storage_cursor.as_ref().map_or(false, |cursor_key| {
                                                match parse_granular_key(cursor_key.as_slice()) {
                                                    Some((ref next_obj, GranularRecord::Entry(next_entry)))
                                                        if next_obj == normalized_id
                                                            && next_entry.starts_with(prefix) =>
                                                    {
                                                        true
                                                    }
                                                    _ => false,
                                                }
                                            })
                                        };
                                    }

                                    more_available = additional_matches;
                                    break 'outer;
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to deserialize entry during list: {}",
                                    e
                                );
                            }
                        }
                    }
                    Some((ref obj_id, GranularRecord::Metadata))
                        if obj_id == normalized_id =>
                    {
                        continue;
                    }
                    _ => continue,
                }
            }

            if let Some(next_cursor_key) = storage_cursor.take() {
                scan_start = next_cursor_key;
                skip_cursor_match = false;
            } else {
                break;
            }
        }

        if prefix_exhausted {
            more_available = false;
        }

        debug!(
            "Listed {} entries for object {} (more_available={}, prefix={:?})",
            entries.len(),
            normalized_id,
            more_available,
            key_prefix
        );

        Ok(EntryListResult {
            entries,
            next_cursor: if more_available {
                last_emitted_key
            } else {
                None
            },
        })
    }

    #[instrument(skip(self, values), fields(obj_id = normalized_id, count = values.len()))]
    async fn batch_set_entries(
        &self,
        normalized_id: &str,
        values: HashMap<String, ObjectVal>,
        expected_version: Option<u64>,
    ) -> Result<u64, ShardError> {
        if values.is_empty() {
            // No-op: do not bump version or emit events
            let meta =
                self.get_metadata(normalized_id).await?.unwrap_or_default();
            return Ok(meta.object_version);
        }
        // Get current metadata
        let mut metadata =
            self.get_metadata(normalized_id).await?.unwrap_or_default();
        let version_before_batch = metadata.object_version;

        // Check expected version for CAS
        if let Some(expected) = expected_version {
            if metadata.object_version != expected {
                return Err(ShardError::VersionMismatch {
                    expected,
                    actual: metadata.object_version,
                });
            }
        }

        // Increment version once for the entire batch
        metadata.increment_version();
        let new_version = metadata.object_version;

        // Snapshot keys for context
        let changed_keys: Vec<String> = values.keys().cloned().collect();
        let v2_mode = std::env::var("ODGM_EVENT_PIPELINE_V2")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);
        let mut pre_exist: HashMap<String, bool> = HashMap::new();
        if v2_mode {
            for k in &changed_keys {
                let sk = build_entry_key(normalized_id, k);
                let exists = self
                    .app_storage
                    .exists(&sk)
                    .await
                    .map_err(ShardError::from)?;
                pre_exist.insert(k.clone(), exists);
            }
        }

        // Write all entries (note: this is simplified - in production, we'd want
        // to batch these into a single Raft log entry for true atomicity)
        for (key, value) in values.into_iter() {
            let storage_key = build_entry_key(normalized_id, &key);
            // Encode each ObjectVal directly
            let value_bytes = bincode::serde::encode_to_vec(
                &value,
                bincode::config::standard(),
            )
            .map_err(|e| ShardError::SerializationError(e.to_string()))?;

            let operation = Operation::Write(WriteOperation {
                key: StorageValue::from(storage_key),
                value: StorageValue::from(value_bytes),
                ..Default::default()
            });
            let request = ShardRequest::from_operation(operation, 0); // TODO: Get proper node_id

            self.replication
                .replicate_write(request)
                .await
                .map_err(ShardError::from)?;

            self.metrics.inc_entry_writes();
        }

        // Update metadata with new version
        self.set_metadata(normalized_id, metadata).await?;

        debug!("Batch set completed with new version {}", new_version);
        if v2_mode {
            let mut changed: Vec<ChangedKey> =
                Vec::with_capacity(changed_keys.len());
            for k in &changed_keys {
                let existed = *pre_exist.get(k).unwrap_or(&false);
                changed.push(ChangedKey {
                    key_canonical: k.clone(),
                    action: if existed {
                        MutAction::Update
                    } else {
                        MutAction::Create
                    },
                });
            }
            let event_cfg = {
                let key_ev = string_object_event_config_key(normalized_id);
                match self.app_storage.get(&key_ev).await {
                    Ok(Some(val)) => ObjectEvent::decode(val.as_slice())
                        .ok()
                        .map(|ev| std::sync::Arc::new(ev)),
                    _ => None,
                }
            };
            let ctx = MutationContext::new(
                normalized_id.to_string(),
                self.class_id().to_string(),
                self.partition_id_u16(),
                version_before_batch,
                new_version,
                changed,
            )
            .with_event_config(event_cfg);
            if let Some(v2) = &self.v2_dispatcher {
                v2.try_send(ctx);
            }
        } else if let Some(bridge) = &self.bridge_dispatcher {
            let evt = build_bridge_event(
                self.class_id(),
                self.partition_id_u16(),
                normalized_id,
                new_version,
                changed_keys,
                bridge,
            );
            if bridge.try_send(evt) {
                self.metrics.inc_bridge_emitted();
            } else {
                self.metrics.inc_bridge_dropped();
            }
        }
        Ok(new_version)
    }

    #[instrument(skip(self, keys), fields(obj_id = normalized_id, count = keys.len()))]
    async fn batch_delete_entries(
        &self,
        normalized_id: &str,
        keys: Vec<String>,
    ) -> Result<(), ShardError> {
        let v2_mode = std::env::var("ODGM_EVENT_PIPELINE_V2")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);
        // Track which keys actually existed (for idempotent suppression)
        let mut deleted_count = 0;
        let mut actually_deleted: Vec<String> = Vec::new();

        for key in &keys {
            let storage_key = build_entry_key(normalized_id, &key);

            // Check if exists
            let exists = self
                .app_storage
                .exists(&storage_key)
                .await
                .map_err(ShardError::from)?;

            if exists {
                let operation = Operation::Delete(DeleteOperation {
                    key: StorageValue::from(storage_key),
                });
                let request = ShardRequest::from_operation(operation, 0); // TODO: Get proper node_id

                self.replication
                    .replicate_write(request)
                    .await
                    .map_err(ShardError::from)?;

                deleted_count += 1;
                actually_deleted.push(key.clone());
                self.metrics.inc_entry_deletes();
            }
        }

        // Increment version only if at least one entry was deleted
        if deleted_count > 0 {
            let mut metadata =
                self.get_metadata(normalized_id).await?.unwrap_or_default();
            let version_before = metadata.object_version;
            metadata.increment_version();
            let version_after = metadata.object_version;
            self.set_metadata(normalized_id, metadata).await?;
            if v2_mode {
                // Enqueue delete mutation context (idempotent deletions excluded)
                let changed: Vec<ChangedKey> = actually_deleted
                    .iter()
                    .map(|k| ChangedKey {
                        key_canonical: k.clone(),
                        action: MutAction::Delete,
                    })
                    .collect();
                if !changed.is_empty() {
                    let event_cfg = {
                        let key_ev =
                            string_object_event_config_key(normalized_id);
                        match self.app_storage.get(&key_ev).await {
                            Ok(Some(val)) => {
                                ObjectEvent::decode(val.as_slice())
                                    .ok()
                                    .map(|ev| std::sync::Arc::new(ev))
                            }
                            _ => None,
                        }
                    };
                    let ctx = MutationContext::new(
                        normalized_id.to_string(),
                        self.class_id().to_string(),
                        self.partition_id_u16(),
                        version_before,
                        version_after,
                        changed,
                    )
                    .with_event_config(event_cfg);
                    if let Some(v2) = &self.v2_dispatcher {
                        v2.try_send(ctx);
                    }
                }
            }
        }

        debug!("Batch deleted {} entries", deleted_count);
        if deleted_count > 0 {
            if let Some(bridge) = &self.bridge_dispatcher {
                let evt = build_bridge_event(
                    self.class_id(),
                    self.partition_id_u16(),
                    normalized_id,
                    self.get_metadata(normalized_id)
                        .await?
                        .unwrap_or_default()
                        .object_version,
                    keys.clone(), // include all requested keys (deleted or not) - simplifies
                    bridge,
                );
                if bridge.try_send(evt) {
                    self.metrics.inc_bridge_emitted();
                } else {
                    self.metrics.inc_bridge_dropped();
                }
            }
        }
        Ok(())
    }

    #[instrument(skip(self), fields(obj_id = normalized_id))]
    async fn delete_object_granular(
        &self,
        normalized_id: &str,
    ) -> Result<(), ShardError> {
        // Get prefix for all records of this object
        let object_prefix = build_object_prefix(normalized_id);

        // Use delete_range for efficient bulk deletion
        let start = object_prefix.clone();
        let mut end = object_prefix.clone();
        // Increment last byte to get exclusive upper bound
        if let Some(last) = end.last_mut() {
            *last = last.saturating_add(1);
        } else {
            end.push(1);
        }

        let range = start..end;

        self.app_storage
            .delete_range(range)
            .await
            .map_err(ShardError::from)?;

        debug!("Deleted entire object {}", normalized_id);
        Ok(())
    }

    #[instrument(skip(self), fields(obj_id = normalized_id))]
    async fn count_entries(
        &self,
        normalized_id: &str,
    ) -> Result<usize, ShardError> {
        let mut total = 0usize;
        let mut cursor: Option<Vec<u8>> = None;

        loop {
            let options = EntryListOptions {
                key_prefix: None,
                limit: MAX_LIST_LIMIT,
                cursor: cursor.clone(),
            };

            let page = self.list_entries(normalized_id, options).await?;
            total += page.entries.len();

            if let Some(next) = page.next_cursor {
                cursor = Some(next);
            } else {
                break;
            }
        }

        Ok(total)
    }
}

fn compute_object_range_end(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    if let Some(last) = end.last_mut() {
        *last = last.saturating_add(1);
    } else {
        end.push(1);
    }
    end
}
