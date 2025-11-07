//! Unified internal API for object operations (string ID only).
//!
//! Purpose:
//! - Provide a single, reusable layer for CRUD and listing of objects with string IDs.
//! - Eliminate divergence between gRPC and Zenoh paths by centralizing granular logic.
//! - Numeric IDs are treated as strings by callers before invoking these functions.
//!
//! Contracts (inputs/outputs):
//! - All functions accept a shard implementing `ObjectShard` and a normalized string object id.
//! - Errors are returned as `ShardError` and should be propagated to the transport layer as-is.
//! - Versioning semantics:
//!   * set_entry: increments object version by 1.
//!   * upsert_object: increments by 1 when entries are present; metadata-only ensures existence without increment.
//!   * batch_mutate_entries: CAS on expected_version when provided; increments once for sets and once for deletes if both occur.
//! - Empty objects are represented by metadata-only records.
//!
//! Edge cases handled:
//! - Non-existent objects: ensure_exists creates metadata; getters return None.
//! - Delete-only batches: increment version only if at least one key existed and was deleted.
//! - Pagination: list_entries returns entries, next_cursor and the object version snapshot.

use std::collections::HashMap;

use tracing::{instrument, trace};

use super::config::ShardError;
use super::object_trait::ObjectShard;
use crate::granular_key::ObjectMetadata;
use crate::granular_trait::EntryListOptions;
use crate::shard::{ObjectData, ObjectVal};

/// Unified internal API for object operations (string ID–only).
/// This module centralizes granular storage logic so both gRPC and Zenoh layers
/// call the same code paths, reducing divergence and maintenance cost.
/// Numeric object IDs have been deprecated in favor of string IDs.
///
/// Versioning rules:
/// - Upsert of non-empty object increments version once (batch_set_entries_granular).
/// - Metadata-only creation sets initial version = 0 (no increment) unless entries are added.
/// - Setting a single entry via `set_entry` increments version once.
/// - Batch mutate with expected_version performs optimistic CAS if provided.
/// - Deletion increments version and sets tombstone (handled in shard internal delete).
///
/// Empty object semantics:
/// - An object may exist with metadata only (ensure_metadata_exists) representing an empty object.
/// - Reconstruction returns empty `ObjectData` (no entries) when metadata exists and not tombstoned.
///
/// All functions are thin wrappers delegating to the shard's granular trait methods.

#[inline]
fn normalize_id(id: &str) -> &str {
    id
}

/// Get full object (including empty metadata-only object) by string ID.
#[instrument(skip(shard), fields(obj_id = id))]
pub async fn get_object<S: ObjectShard>(
    shard: &S,
    id: &str,
) -> Result<Option<ObjectData>, ShardError> {
    let norm = normalize_id(id);
    shard.reconstruct_object_granular(norm, 1024).await
}

/// Ensure metadata exists creating object if absent. Returns true if created.
#[instrument(skip(shard), fields(obj_id = id))]
pub async fn ensure_exists<S: ObjectShard>(
    shard: &S,
    id: &str,
) -> Result<bool, ShardError> {
    shard.ensure_metadata_exists(normalize_id(id)).await
}

/// Upsert full object replacing existing entries. Empty object -> metadata only creation.
#[instrument(skip(shard, obj), fields(obj_id = id))]
pub async fn upsert_object<S: ObjectShard + ?Sized>(
    shard: &S,
    id: &str,
    obj: ObjectData,
) -> Result<(), ShardError> {
    let norm = normalize_id(id);
    let is_empty = obj.value.is_empty() && obj.str_value.is_empty();
    if is_empty {
        let _ = shard.ensure_metadata_exists(norm).await?; // idempotent
        trace!(obj_id = norm, "metadata-only upsert ok");
        return Ok(());
    }
    shard.set_object_by_str_id(norm, obj).await
}

/// Set or overwrite a single entry; increments version.
#[instrument(skip(shard, val), fields(obj_id = id, key))]
pub async fn set_entry<S: ObjectShard>(
    shard: &S,
    id: &str,
    key: &str,
    val: ObjectVal,
) -> Result<(), ShardError> {
    let norm = normalize_id(id);
    shard.set_entry_granular(norm, key, val).await
}

/// Batch mutate entries: set and delete lists atomically; optional CAS on object version.
/// Returns new version if entries set, otherwise current version (delete-only or no-op).
#[instrument(skip(shard, set_map, delete_keys), fields(obj_id = id, n_set = set_map.len(), n_delete = delete_keys.len()))]
pub async fn batch_mutate_entries<S: ObjectShard + ?Sized>(
    shard: &S,
    id: &str,
    set_map: HashMap<String, ObjectVal>,
    delete_keys: Vec<String>,
    expected_version: Option<u64>,
) -> Result<u64, ShardError> {
    let norm = normalize_id(id);
    // Empty set + deletes handled by separate delete path in gRPC; here we treat as delete-only mutation.
    if set_map.is_empty() && delete_keys.is_empty() {
        // No mutation, return current version.
        let meta = shard.get_metadata_granular(norm).await?;
        return Ok(meta.map(|m| m.object_version).unwrap_or(0));
    }
    let new_version = if !set_map.is_empty() {
        shard
            .batch_set_entries_granular(norm, set_map, expected_version)
            .await?
    } else {
        // Delete only: verify version if provided
        if let Some(expected) = expected_version {
            let current = shard
                .get_metadata_granular(norm)
                .await?
                .map(|m| m.object_version)
                .unwrap_or(0);
            if current != expected {
                return Err(ShardError::VersionMismatch {
                    expected,
                    actual: current,
                });
            }
        }
        // Delete entries individually (no version increment unless at least one existed)
        let mut deleted_any = false;
        for key in delete_keys.iter() {
            if let Some(_existing) = shard.get_entry_granular(norm, key).await?
            {
                shard.delete_entry_granular(norm, key).await?;
                deleted_any = true;
            }
        }
        if deleted_any {
            // Increment version by setting metadata explicitly
            let mut meta = shard
                .get_metadata_granular(norm)
                .await?
                .unwrap_or_else(ObjectMetadata::default);
            let before = meta.object_version;
            meta.increment_version();
            shard.set_metadata_granular(norm, meta.clone()).await?;
            trace!(
                obj_id = norm,
                version_before = before,
                version_after = meta.object_version,
                "delete-only batch incremented version"
            );
            meta.object_version
        } else {
            shard
                .get_metadata_granular(norm)
                .await?
                .map(|m| m.object_version)
                .unwrap_or(0)
        }
    };
    // Apply deletes after sets if both present
    if !delete_keys.is_empty() && new_version > 0 {
        for key in delete_keys.iter() {
            if let Some(_existing) = shard.get_entry_granular(norm, key).await?
            {
                shard.delete_entry_granular(norm, key).await?;
            }
        }
    }
    Ok(new_version)
}

/// List entries with pagination helper.
#[instrument(skip(shard), fields(obj_id = id, limit = options.limit))]
pub async fn list_entries<S: ObjectShard + ?Sized>(
    shard: &S,
    id: &str,
    options: EntryListOptions,
) -> Result<(Vec<(String, ObjectVal)>, Option<Vec<u8>>, u64), ShardError> {
    let norm = normalize_id(id);
    let meta = shard.get_metadata_granular(norm).await?;
    let version = meta.map(|m| m.object_version).unwrap_or(0);
    let result = shard.list_entries_granular(norm, options).await?;
    Ok((result.entries, result.next_cursor, version))
}
