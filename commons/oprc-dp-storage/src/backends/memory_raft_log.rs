use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    RaftHardState, RaftLogEntry, RaftLogEntryType, RaftLogId, RaftLogStats,
    RaftLogStorage, RaftMembership, RaftSnapshotMeta, StorageResult,
};

/// In-memory Raft log storage implementation using BTreeMap for ordering
/// This provides all the Raft state management methods required for consensus
#[derive(Debug, Clone)]
pub struct MemoryRaftLogStorage {
    /// Log entries storage - maps log index to entry
    /// Key: log index, Value: RaftLogEntry
    entries: Arc<RwLock<BTreeMap<u64, RaftLogEntry>>>,

    /// Applied log state - tracks the last log entry that was applied to the state machine
    applied_state: Arc<RwLock<Option<RaftLogId>>>,

    /// Snapshot metadata storage - only the most recent snapshot
    snapshot_meta: Arc<RwLock<Option<RaftSnapshotMeta>>>,

    /// Hard state storage - persistent Raft state
    hard_state: Arc<RwLock<RaftHardState>>,

    /// Membership configuration storage - tracks cluster membership changes
    membership: Arc<RwLock<RaftMembership>>,
}

impl Default for MemoryRaftLogStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryRaftLogStorage {
    /// Create a new in-memory Raft log storage
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(BTreeMap::new())),
            applied_state: Arc::new(RwLock::new(None)),
            snapshot_meta: Arc::new(RwLock::new(None)),
            hard_state: Arc::new(RwLock::new(RaftHardState::default())),
            membership: Arc::new(RwLock::new(RaftMembership::default())),
        }
    }

    /// Clear all stored data (useful for testing)
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();

        let mut applied = self.applied_state.write().await;
        *applied = None;

        let mut snapshot = self.snapshot_meta.write().await;
        *snapshot = None;

        let mut hard = self.hard_state.write().await;
        *hard = RaftHardState::default();

        let mut membership = self.membership.write().await;
        *membership = RaftMembership::default();
    }

    /// Get the number of stored log entries
    pub async fn entry_count(&self) -> usize {
        let entries = self.entries.read().await;
        entries.len()
    }
}

#[async_trait]
impl RaftLogStorage for MemoryRaftLogStorage {
    type Error = crate::StorageError;

    // ========================================================================
    // Log Entry Management
    // ========================================================================

    async fn append_entry(
        &self,
        index: u64,
        term: u64,
        entry: &[u8],
    ) -> Result<(), Self::Error> {
        let mut entries = self.entries.write().await;
        let log_entry = RaftLogEntry {
            index,
            term,
            entry_type: RaftLogEntryType::Normal,
            data: entry.to_vec().into(),
            timestamp: std::time::SystemTime::now(),
            checksum: None,
        };
        entries.insert(index, log_entry);
        Ok(())
    }

    async fn append_entries(
        &self,
        entries: Vec<RaftLogEntry>,
    ) -> Result<(), Self::Error> {
        let mut stored_entries = self.entries.write().await;
        for entry in entries {
            stored_entries.insert(entry.index, entry);
        }
        Ok(())
    }

    async fn get_entry(
        &self,
        index: u64,
    ) -> Result<Option<RaftLogEntry>, Self::Error> {
        let entries = self.entries.read().await;
        Ok(entries.get(&index).cloned())
    }

    async fn get_entries(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<RaftLogEntry>, Self::Error> {
        let entries = self.entries.read().await;
        let mut result = Vec::new();
        for index in start..=end {
            if let Some(entry) = entries.get(&index) {
                result.push(entry.clone());
            }
        }
        Ok(result)
    }

    async fn last_index(&self) -> Result<Option<u64>, Self::Error> {
        let entries = self.entries.read().await;
        Ok(entries.keys().last().copied())
    }

    async fn first_index(&self) -> Result<Option<u64>, Self::Error> {
        let entries = self.entries.read().await;
        Ok(entries.keys().next().copied())
    }

    async fn truncate_from(&self, index: u64) -> Result<(), Self::Error> {
        let mut entries = self.entries.write().await;
        let to_remove: Vec<u64> =
            entries.keys().filter(|&&k| k >= index).copied().collect();
        for key in to_remove {
            entries.remove(&key);
        }
        Ok(())
    }

    async fn truncate_before(&self, index: u64) -> Result<(), Self::Error> {
        let mut entries = self.entries.write().await;
        let to_remove: Vec<u64> =
            entries.keys().filter(|&&k| k < index).copied().collect();
        for key in to_remove {
            entries.remove(&key);
        }
        Ok(())
    }

    // ========================================================================
    // Raft State Management
    // ========================================================================

    async fn get_hard_state(&self) -> Result<RaftHardState, Self::Error> {
        let hard_state = self.hard_state.read().await;
        Ok(hard_state.clone())
    }

    async fn save_hard_state(
        &self,
        hard_state: &RaftHardState,
    ) -> Result<(), Self::Error> {
        let mut stored_hard_state = self.hard_state.write().await;
        *stored_hard_state = hard_state.clone();
        Ok(())
    }

    async fn save_applied_state(
        &self,
        log_id: Option<&RaftLogId>,
    ) -> Result<(), Self::Error> {
        let mut applied = self.applied_state.write().await;
        *applied = log_id.cloned();
        Ok(())
    }

    async fn get_applied_state(
        &self,
    ) -> Result<Option<RaftLogId>, Self::Error> {
        let applied = self.applied_state.read().await;
        Ok(applied.clone())
    }

    async fn applied_state(
        &self,
    ) -> StorageResult<(Option<RaftLogId>, RaftMembership)> {
        let applied = self.applied_state.read().await;
        let membership = self.membership.read().await;
        Ok((applied.clone(), membership.clone()))
    }

    async fn save_membership(
        &self,
        membership: &RaftMembership,
    ) -> StorageResult<()> {
        let mut stored_membership = self.membership.write().await;
        *stored_membership = membership.clone();
        Ok(())
    }

    async fn install_snapshot_state(
        &self,
        meta: &RaftSnapshotMeta,
    ) -> StorageResult<()> {
        // Store the snapshot metadata
        let mut snapshot = self.snapshot_meta.write().await;
        *snapshot = Some(meta.clone());

        // Update applied state to reflect the snapshot
        let mut applied = self.applied_state.write().await;
        *applied = meta.last_log_id.clone();

        let mut membership = self.membership.write().await;
        *membership = meta.last_membership.clone();

        Ok(())
    }

    async fn last_snapshot_meta(
        &self,
    ) -> Result<Option<RaftSnapshotMeta>, Self::Error> {
        let snapshot = self.snapshot_meta.read().await;
        Ok(snapshot.clone())
    }

    async fn save_snapshot_meta(
        &self,
        meta: &RaftSnapshotMeta,
    ) -> Result<(), Self::Error> {
        let mut snapshot = self.snapshot_meta.write().await;
        *snapshot = Some(meta.clone());
        Ok(())
    }

    async fn compact_to(&self, index: u64) -> Result<(), Self::Error> {
        // Remove log entries up to and including the specified index
        self.truncate_before(index + 1).await
    }

    // ========================================================================
    // Statistics and Maintenance
    // ========================================================================

    async fn log_stats(&self) -> Result<RaftLogStats, Self::Error> {
        let entries = self.entries.read().await;
        let first_index = entries.keys().next().copied();
        let last_index = entries.keys().last().copied();
        let entry_count = entries.len() as u64;
        let total_size_bytes =
            entries.values().map(|entry| entry.data.len() as u64).sum();

        Ok(RaftLogStats {
            first_index,
            last_index,
            entry_count,
            total_size_bytes,
            compacted_entries: 0,
            write_throughput_ops_per_sec: 0.0,
            avg_entry_size_bytes: if entry_count > 0 {
                total_size_bytes as f64 / entry_count as f64
            } else {
                0.0
            },
        })
    }

    async fn sync(&self) -> Result<(), Self::Error> {
        // No-op for in-memory storage
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_entry_operations() {
        let storage = MemoryRaftLogStorage::new();

        // Initially should be empty
        assert_eq!(storage.entry_count().await, 0);
        assert!(storage.first_index().await.unwrap().is_none());
        assert!(storage.last_index().await.unwrap().is_none());

        // Append single entry
        storage.append_entry(1, 1, b"entry1").await.unwrap();
        assert_eq!(storage.entry_count().await, 1);
        assert_eq!(storage.first_index().await.unwrap(), Some(1));
        assert_eq!(storage.last_index().await.unwrap(), Some(1));

        // Get entry
        let entry = storage.get_entry(1).await.unwrap().unwrap();
        assert_eq!(entry.index, 1);
        assert_eq!(entry.term, 1);
        assert_eq!(entry.data.as_slice(), b"entry1");

        // Append multiple entries
        let entries = vec![
            RaftLogEntry {
                index: 2,
                term: 1,
                entry_type: RaftLogEntryType::Normal,
                data: b"entry2".to_vec().into(),
                timestamp: std::time::SystemTime::now(),
                checksum: None,
            },
            RaftLogEntry {
                index: 3,
                term: 2,
                entry_type: RaftLogEntryType::Normal,
                data: b"entry3".to_vec().into(),
                timestamp: std::time::SystemTime::now(),
                checksum: None,
            },
        ];
        storage.append_entries(entries).await.unwrap();
        assert_eq!(storage.entry_count().await, 3);

        // Get range of entries
        let range = storage.get_entries(1, 3).await.unwrap();
        assert_eq!(range.len(), 3);
        assert_eq!(range[0].data.as_slice(), b"entry1");
        assert_eq!(range[1].data.as_slice(), b"entry2");
        assert_eq!(range[2].data.as_slice(), b"entry3");
    }

    #[tokio::test]
    async fn test_truncation() {
        let storage = MemoryRaftLogStorage::new();

        // Add some entries
        for i in 1..=5 {
            storage
                .append_entry(i, 1, format!("entry{}", i).as_bytes())
                .await
                .unwrap();
        }
        assert_eq!(storage.entry_count().await, 5);

        // Truncate from index 3 (removes 3, 4, 5)
        storage.truncate_from(3).await.unwrap();
        assert_eq!(storage.entry_count().await, 2);
        assert_eq!(storage.last_index().await.unwrap(), Some(2));

        // Add more entries
        for i in 3..=6 {
            storage
                .append_entry(i, 2, format!("new_entry{}", i).as_bytes())
                .await
                .unwrap();
        }
        assert_eq!(storage.entry_count().await, 6);

        // Truncate before index 3 (removes 1, 2)
        storage.truncate_before(3).await.unwrap();
        assert_eq!(storage.entry_count().await, 4);
        assert_eq!(storage.first_index().await.unwrap(), Some(3));
    }

    #[tokio::test]
    async fn test_applied_state() {
        let storage = MemoryRaftLogStorage::new();

        // Initially should be empty
        let applied = storage.get_applied_state().await.unwrap();
        assert!(applied.is_none());

        // Save applied state
        let log_id = RaftLogId {
            term: 1,
            index: 100,
        };
        storage.save_applied_state(Some(&log_id)).await.unwrap();

        // Retrieve and verify
        let retrieved = storage.get_applied_state().await.unwrap();
        assert_eq!(retrieved, Some(log_id));
    }

    #[tokio::test]
    async fn test_snapshot_management() {
        let storage = MemoryRaftLogStorage::new();

        // Initially should be empty
        let snapshot = storage.last_snapshot_meta().await.unwrap();
        assert!(snapshot.is_none());

        // Create test snapshot metadata
        let snapshot_meta = RaftSnapshotMeta {
            last_log_id: Some(RaftLogId {
                term: 2,
                index: 200,
            }),
            last_membership: RaftMembership::default(),
            snapshot_id: "test_snapshot_1".to_string(),
            created_at: std::time::SystemTime::now(),
            size_bytes: 1024,
            checksum: Some("abc123".to_string()),
        };

        // Install snapshot
        storage
            .install_snapshot_state(&snapshot_meta)
            .await
            .unwrap();

        // Verify snapshot was stored
        let retrieved = storage.last_snapshot_meta().await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().size_bytes, 1024);

        // Verify applied state was updated
        let applied = storage.get_applied_state().await.unwrap();
        assert_eq!(
            applied,
            Some(RaftLogId {
                term: 2,
                index: 200
            })
        );
    }

    #[tokio::test]
    async fn test_log_stats() {
        let storage = MemoryRaftLogStorage::new();

        // Initially should be empty
        let stats = storage.log_stats().await.unwrap();
        assert!(stats.first_index.is_none());
        assert!(stats.last_index.is_none());
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.total_size_bytes, 0);

        // Add some entries
        for i in 10..=15 {
            let data = format!("entry_{}", i);
            storage.append_entry(i, 1, data.as_bytes()).await.unwrap();
        }

        let stats = storage.log_stats().await.unwrap();
        assert_eq!(stats.first_index, Some(10));
        assert_eq!(stats.last_index, Some(15));
        assert_eq!(stats.entry_count, 6);
        assert!(stats.total_size_bytes > 0);
    }
}
