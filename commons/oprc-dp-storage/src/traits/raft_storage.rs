use async_trait::async_trait;
use std::error::Error;

/// Specialized trait for Raft log storage - append-only, high-throughput writes
#[async_trait]
pub trait RaftLogStorage: Send + Sync + Clone {
    type Error: Error + Send + Sync + 'static;

    // ========================================================================
    // Log Entry Management
    // ========================================================================

    /// Append a single log entry at the specified index
    async fn append_entry(
        &self,
        index: u64,
        term: u64,
        entry: &[u8],
    ) -> Result<(), Self::Error>;

    /// Append multiple log entries atomically (batch operation)
    async fn append_entries(
        &self,
        entries: Vec<crate::RaftLogEntry>,
    ) -> Result<(), Self::Error>;

    /// Get a log entry by index
    async fn get_entry(
        &self,
        index: u64,
    ) -> Result<Option<crate::RaftLogEntry>, Self::Error>;

    /// Get a range of log entries efficiently
    async fn get_entries(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<crate::RaftLogEntry>, Self::Error>;

    /// Get the last log index (highest index stored)
    async fn last_index(&self) -> Result<Option<u64>, Self::Error>;

    /// Get the first log index (lowest index stored, after compaction)
    async fn first_index(&self) -> Result<Option<u64>, Self::Error>;

    /// Truncate log entries from index onwards (for log repair)
    async fn truncate_from(&self, index: u64) -> Result<(), Self::Error>;

    /// Truncate log entries before index (for log compaction)
    async fn truncate_before(&self, index: u64) -> Result<(), Self::Error>;

    // ========================================================================
    // Raft State Management (for snapshot integration)
    // ========================================================================

    /// Get the current applied state (last applied log ID and membership)
    /// This is critical for snapshot creation and consistency
    async fn applied_state(
        &self,
    ) -> Result<(Option<crate::RaftLogId>, crate::RaftMembership), Self::Error>;

    /// Save/update the current membership configuration
    /// Called when membership changes are committed
    async fn save_membership(
        &self,
        membership: &crate::RaftMembership,
    ) -> Result<(), Self::Error>;

    /// Get the current Raft hard state (term, vote)
    async fn get_hard_state(&self) -> Result<crate::RaftHardState, Self::Error>;

    /// Save Raft hard state (term, vote) atomically
    async fn save_hard_state(
        &self,
        hard_state: &crate::RaftHardState,
    ) -> Result<(), Self::Error>;

    /// Update the last applied log ID (marks progress of state machine)
    async fn save_applied_state(
        &self,
        log_id: Option<&crate::RaftLogId>,
    ) -> Result<(), Self::Error>;

    /// Get the last applied log ID
    async fn get_applied_state(&self)
        -> Result<Option<crate::RaftLogId>, Self::Error>;

    // ========================================================================
    // Snapshot State Management (for snapshot integration)
    // ========================================================================

    /// Update log storage state after snapshot installation
    /// This typically involves updating applied state and potentially compacting logs
    async fn install_snapshot_state(
        &self,
        meta: &crate::RaftSnapshotMeta,
    ) -> Result<(), Self::Error>;

    /// Get the last snapshot metadata
    async fn last_snapshot_meta(
        &self,
    ) -> Result<Option<crate::RaftSnapshotMeta>, Self::Error>;

    /// Save snapshot metadata (when snapshot is created or installed)
    async fn save_snapshot_meta(
        &self,
        meta: &crate::RaftSnapshotMeta,
    ) -> Result<(), Self::Error>;

    /// Mark logs as compacted up to a certain index (post-snapshot)
    /// This is safe log cleanup after successful snapshot
    async fn compact_to(&self, index: u64) -> Result<(), Self::Error>;

    // ========================================================================
    // Storage Management
    // ========================================================================

    /// Get log storage statistics and health information
    async fn log_stats(&self) -> Result<crate::RaftLogStats, Self::Error>;

    /// Force flush any buffered writes to durable storage
    async fn sync(&self) -> Result<(), Self::Error>;
}
