//! Storage trait extensions for granular (per-entry) operations.
//! Phase B: Define the EntryStore trait for per-entry CRUD operations.

use crate::granular_key::ObjectMetadata;
use crate::shard::ObjectVal;
use crate::shard::unified::config::ShardError;
use async_trait::async_trait;
use std::collections::HashMap;

/// Extended trait for granular per-entry storage operations.
/// Implementations provide fine-grained access to individual object entries.
#[async_trait]
pub trait EntryStore: Send + Sync {
    /// Get metadata for an object (version, tombstone, attributes).
    /// Returns None if object does not exist.
    async fn get_metadata(
        &self,
        normalized_id: &str,
    ) -> Result<Option<ObjectMetadata>, ShardError>;

    /// Set metadata for an object.
    async fn set_metadata(
        &self,
        normalized_id: &str,
        metadata: ObjectMetadata,
    ) -> Result<(), ShardError>;

    /// Get a single entry value by key.
    /// Returns None if the object or entry does not exist.
    /// Key is always a string (numeric keys should be converted via `numeric_key_to_string`).
    async fn get_entry(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<Option<ObjectVal>, ShardError>;

    /// Set a single entry value.
    /// Creates the object if it doesn't exist (with default metadata).
    /// Increments object_version automatically.
    async fn set_entry(
        &self,
        normalized_id: &str,
        key: &str,
        value: ObjectVal,
    ) -> Result<(), ShardError>;

    /// Delete a single entry.
    /// Returns Ok(()) even if entry doesn't exist (idempotent).
    /// Increments object_version only if entry was actually removed.
    async fn delete_entry(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<(), ShardError>;

    /// List all entries for an object, optionally filtered by key prefix.
    /// Returns iterator of (key, value) pairs.
    /// Prefix matching is simple string prefix (e.g., "user:" matches "user:123").
    async fn list_entries(
        &self,
        normalized_id: &str,
        key_prefix: Option<&str>,
    ) -> Result<Vec<(String, ObjectVal)>, ShardError>;

    /// Batch set multiple entries atomically.
    /// All entries are set with the same incremented object_version.
    /// Expected_version implements optimistic concurrency control (CAS):
    /// - If Some(v) and current version != v, returns ShardError::TransactionError.
    /// - If None, no version check is performed.
    async fn batch_set_entries(
        &self,
        normalized_id: &str,
        values: HashMap<String, ObjectVal>,
        expected_version: Option<u64>,
    ) -> Result<u64, ShardError>; // Returns new version

    /// Batch delete multiple entries atomically.
    /// All entries are deleted with the same incremented object_version.
    async fn batch_delete_entries(
        &self,
        normalized_id: &str,
        keys: Vec<String>,
    ) -> Result<(), ShardError>;

    /// Delete entire object (metadata + all entries).
    /// This is more efficient than deleting entries individually.
    async fn delete_object_granular(
        &self,
        normalized_id: &str,
    ) -> Result<(), ShardError>;

    /// Count total entries in an object.
    async fn count_entries(
        &self,
        normalized_id: &str,
    ) -> Result<usize, ShardError>;
}

/// Transaction support for batch operations.
/// Allows grouping multiple granular operations with rollback capability.
#[async_trait(?Send)]
pub trait EntryStoreTransaction {
    /// Get entry within transaction.
    async fn get_entry(
        &self,
        normalized_id: &str,
        key: &str,
    ) -> Result<Option<ObjectVal>, ShardError>;

    /// Set entry within transaction (buffered until commit).
    async fn set_entry(
        &mut self,
        normalized_id: &str,
        key: &str,
        value: ObjectVal,
    ) -> Result<(), ShardError>;

    /// Delete entry within transaction (buffered until commit).
    async fn delete_entry(
        &mut self,
        normalized_id: &str,
        key: &str,
    ) -> Result<(), ShardError>;

    /// Get current object version within transaction.
    async fn get_version(
        &self,
        normalized_id: &str,
    ) -> Result<Option<u64>, ShardError>;

    /// Commit all buffered operations atomically.
    async fn commit(self: Box<Self>) -> Result<(), ShardError>;

    /// Rollback/abort transaction.
    async fn rollback(self: Box<Self>) -> Result<(), ShardError>;
}

/// Helper struct for batch operation results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSetResult {
    /// New object version after batch.
    pub new_version: u64,
    /// Number of entries set.
    pub entries_set: usize,
}

/// Helper struct for listing with pagination.
#[derive(Debug, Clone)]
pub struct EntryListOptions {
    /// Optional key prefix filter.
    pub key_prefix: Option<String>,
    /// Maximum entries to return.
    pub limit: usize,
    /// Opaque cursor for pagination.
    pub cursor: Option<Vec<u8>>,
}

impl Default for EntryListOptions {
    fn default() -> Self {
        Self {
            key_prefix: None,
            limit: 100,
            cursor: None,
        }
    }
}

/// Result of a paginated list operation.
#[derive(Debug, Clone)]
pub struct EntryListResult {
    /// Retrieved entries.
    pub entries: Vec<(String, ObjectVal)>,
    /// Cursor for next page (None if no more results).
    pub next_cursor: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_list_options_default() {
        let opts = EntryListOptions::default();
        assert_eq!(opts.limit, 100);
        assert!(opts.key_prefix.is_none());
        assert!(opts.cursor.is_none());
    }

    #[test]
    fn test_batch_set_result() {
        let result = BatchSetResult {
            new_version: 42,
            entries_set: 10,
        };
        assert_eq!(result.new_version, 42);
        assert_eq!(result.entries_set, 10);
    }
}
