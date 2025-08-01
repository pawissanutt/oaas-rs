use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use openraft::storage::{
    LogFlushed, LogState, RaftLogStorage, RaftStateMachine, Snapshot,
};
use openraft::{
    Entry, EntryPayload, LogId, RaftLogReader, RaftSnapshotBuilder,
    RaftStorage, SnapshotMeta, StorageError, StorageIOError, Vote,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::raft_types::*;
use crate::replication::{Operation, ReplicationError, ShardRequest};
use oprc_dp_storage::{
    AppendOnlyLogStorage, ApplicationDataStorage, CompressedSnapshotStorage,
    EnhancedApplicationStorage, MemoryStorage,
    RaftLogStorage as RaftLogStorageTrait, RaftSnapshotStorage, StorageBackend,
    StorageValue,
};

/// OpenRaft storage implementation following the flexible storage design
/// Uses separate storage layers for optimal performance:
/// - Log storage: Append-only, optimized for sequential writes
/// - Snapshot storage: Compressed, optimized for bulk operations  
/// - Application storage: Full-featured, optimized for random access
#[derive(Debug)]
pub struct OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
    /// Raft log storage - optimized for append-only sequential writes
    log_storage: L,

    /// Vote information storage (using application storage)
    vote: Arc<RwLock<Option<Vote<NodeId>>>>,

    /// Snapshot storage - optimized for compression and bulk operations
    snapshot_storage: S,

    /// Current snapshot metadata
    snapshot_meta: Arc<RwLock<Option<SnapshotMeta<NodeId, RaftNode>>>>,

    /// Application state machine storage - optimized for random access
    app_storage: A,

    /// Applied log index tracking
    last_applied: Arc<RwLock<Option<LogId<NodeId>>>>,

    /// Current membership configuration
    last_membership: Arc<RwLock<openraft::StoredMembership<NodeId>>>,
}

impl<L, S, A> OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
    pub fn new(log_storage: L, snapshot_storage: S, app_storage: A) -> Self {
        Self {
            log_storage,
            vote: Arc::new(RwLock::new(None)),
            snapshot_storage,
            snapshot_meta: Arc::new(RwLock::new(None)),
            app_storage,
            last_applied: Arc::new(RwLock::new(None)),
            last_membership: Arc::new(RwLock::new(
                openraft::StoredMembership::default(),
            )),
        }
    }

    /// Create a default storage setup using memory backends for testing
    pub async fn new_memory() -> Result<Self, ReplicationError> {
        let memory_backend = MemoryStorage::new_default()
            .map_err(|e| ReplicationError::StorageError(e.to_string()))?;

        let log_storage = AppendOnlyLogStorage::new(memory_backend.clone())
            .await
            .map_err(|e| ReplicationError::StorageError(e.to_string()))?;

        let snapshot_storage = CompressedSnapshotStorage::new_memory()
            .await
            .map_err(|e| ReplicationError::StorageError(e.to_string()))?;

        let app_storage = EnhancedApplicationStorage::new(memory_backend)
            .map_err(|e| ReplicationError::StorageError(e.to_string()))?;

        Ok(Self::new(log_storage, snapshot_storage, app_storage))
    }

    /// Apply a Raft request to the application state machine
    async fn apply_request(
        &self,
        request: &RaftRequest,
    ) -> Result<RaftResponse, ReplicationError> {
        match &request.shard_request.operation {
            Operation::Write(write_op) => {
                self.app_storage
                    .put(write_op.key.as_bytes(), write_op.value.clone())
                    .await
                    .map_err(|e| {
                        ReplicationError::StorageError(e.to_string())
                    })?;

                Ok(RaftResponse {
                    success: true,
                    message: "Write successful".to_string(),
                    data: None,
                })
            }
            Operation::Read(read_op) => {
                let value = self
                    .app_storage
                    .get(read_op.key.as_bytes())
                    .await
                    .map_err(|e| {
                        ReplicationError::StorageError(e.to_string())
                    })?;

                let data = value.map(|v| v.into_bytes());

                Ok(RaftResponse {
                    success: true,
                    message: "Read successful".to_string(),
                    data,
                })
            }
            Operation::Delete(delete_op) => {
                self.app_storage
                    .delete(delete_op.key.as_bytes())
                    .await
                    .map_err(|e| {
                        ReplicationError::StorageError(e.to_string())
                    })?;

                Ok(RaftResponse {
                    success: true,
                    message: "Delete successful".to_string(),
                    data: None,
                })
            }
            Operation::Batch(operations) => {
                for op in operations {
                    let single_request = RaftRequest {
                        shard_request: ShardRequest {
                            operation: op.clone(),
                            timestamp: request.shard_request.timestamp,
                            source_node: request.shard_request.source_node,
                            request_id: request
                                .shard_request
                                .request_id
                                .clone(),
                        },
                    };
                    self.apply_request(&single_request).await?;
                }

                Ok(RaftResponse {
                    success: true,
                    message: format!(
                        "Batch of {} operations successful",
                        operations.len()
                    ),
                    data: None,
                })
            }
        }
    }

    /// Create a snapshot of the current application state
    async fn build_application_snapshot(
        &self,
    ) -> Result<RaftSnapshot, StorageError<NodeId>> {
        let last_applied = *self.last_applied.read().await;
        let last_membership = self.last_membership.read().await.clone();

        // Export all application data for the snapshot
        let app_data = self
            .app_storage
            .export_all()
            .await
            .map_err(|e| StorageIOError::write_snapshot(None, &e))?;

        let snapshot_data = bincode::serialize(&app_data)
            .map_err(|e| StorageIOError::write_snapshot(None, &e))?;

        let meta = RaftSnapshotMeta {
            last_applied: last_applied.map(|id| id.index).unwrap_or(0),
            last_applied_term: last_applied
                .map(|id| id.leader_id.term)
                .unwrap_or_default(),
            last_membership,
            created_at: std::time::SystemTime::now(),
            size_bytes: snapshot_data.len() as u64,
        };

        Ok(RaftSnapshot {
            meta,
            data: snapshot_data,
        })
    }

    /// Convert Raft log entry to our internal format
    fn to_raft_log_entry(
        &self,
        entry: &Entry<RaftRequest>,
    ) -> oprc_dp_storage::RaftLogEntry {
        use oprc_dp_storage::{RaftLogEntry, RaftLogEntryType};

        let entry_type = match &entry.payload {
            EntryPayload::Blank => RaftLogEntryType::NoOp,
            EntryPayload::Normal(_) => RaftLogEntryType::Normal,
            EntryPayload::Membership(_) => RaftLogEntryType::Configuration,
        };

        let data = bincode::serialize(&entry.payload).unwrap_or_default();

        RaftLogEntry {
            index: entry.log_id.index,
            term: entry.log_id.leader_id.term,
            entry_type,
            data,
            timestamp: std::time::SystemTime::now(),
            checksum: None,
        }
    }

    /// Convert from our internal format to Raft log entry
    fn from_raft_log_entry(
        &self,
        log_entry: oprc_dp_storage::RaftLogEntry,
    ) -> Option<Entry<RaftRequest>> {
        let payload: EntryPayload<RaftRequest> =
            bincode::deserialize(&log_entry.data).ok()?;

        Some(Entry {
            log_id: LogId {
                leader_id: openraft::CommittedLeaderId::new(
                    log_entry.term,
                    NodeId::default(),
                ),
                index: log_entry.index,
            },
            payload,
        })
    }
}

#[async_trait]
impl<L, S, A> RaftLogReader<RaftRequest> for OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
    async fn get_log_state(
        &mut self,
    ) -> Result<LogState<RaftRequest>, StorageError<NodeId>> {
        let first_index = self
            .log_storage
            .first_index()
            .await
            .map_err(|e| StorageError::read(&e))?;
        let last_index = self
            .log_storage
            .last_index()
            .await
            .map_err(|e| StorageError::read(&e))?;

        let last_log_id = if let Some(last_idx) = last_index {
            if let Some(entry) = self
                .log_storage
                .get_entry(last_idx)
                .await
                .map_err(|e| StorageError::read(&e))?
            {
                Some(LogId {
                    leader_id: openraft::CommittedLeaderId::new(
                        entry.term,
                        NodeId::default(),
                    ),
                    index: entry.index,
                })
            } else {
                None
            }
        } else {
            None
        };

        let last_purged_log_id = first_index
            .map(|idx| {
                if idx > 1 {
                    Some(LogId {
                        leader_id: openraft::CommittedLeaderId::new(
                            0,
                            NodeId::default(),
                        ),
                        index: idx - 1,
                    })
                } else {
                    None
                }
            })
            .flatten();

        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn try_get_log_entries<
        RB: openraft::RaftLogReaderExt<RaftRequest> + ?Sized,
    >(
        &mut self,
        range: std::ops::RangeInclusive<u64>,
    ) -> Result<Vec<Entry<RaftRequest>>, StorageError<NodeId>> {
        let start = *range.start();
        let end = *range.end();

        let log_entries = self
            .log_storage
            .get_entries(start, end + 1)
            .await
            .map_err(|e| StorageError::read(&e))?;

        let mut entries = Vec::new();
        for log_entry in log_entries {
            if let Some(entry) = self.from_raft_log_entry(log_entry) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

#[async_trait]
impl<L, S, A> RaftLogStorage<RaftRequest> for OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
    type LogReader = Self;

    async fn get_log_reader(&mut self) -> Self::LogReader {
        Self {
            log_storage: self.log_storage.clone(),
            vote: self.vote.clone(),
            snapshot_storage: self.snapshot_storage.clone(),
            snapshot_meta: self.snapshot_meta.clone(),
            app_storage: self.app_storage.clone(),
            last_applied: self.last_applied.clone(),
            last_membership: self.last_membership.clone(),
        }
    }

    async fn append_to_log<I>(
        &mut self,
        entries: I,
    ) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<RaftRequest>> + Send,
    {
        let log_entries: Vec<_> = entries
            .into_iter()
            .map(|entry| self.to_raft_log_entry(&entry))
            .collect();

        self.log_storage
            .append_entries(log_entries)
            .await
            .map_err(|e| StorageError::write(&e))?;

        Ok(())
    }

    async fn delete_conflict_logs_since(
        &mut self,
        log_id: LogId<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        self.log_storage
            .truncate_from(log_id.index)
            .await
            .map_err(|e| StorageError::write(&e))?;
        Ok(())
    }

    async fn purge_logs_upto(
        &mut self,
        log_id: LogId<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        self.log_storage
            .truncate_before(log_id.index + 1)
            .await
            .map_err(|e| StorageError::write(&e))?;
        Ok(())
    }

    async fn read_vote(
        &mut self,
    ) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        Ok(*self.vote.read().await)
    }

    async fn save_vote(
        &mut self,
        vote: &Vote<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        *self.vote.write().await = Some(*vote);
        Ok(())
    }

    async fn flush(
        &mut self,
        _log_id: LogId<NodeId>,
    ) -> Result<LogFlushed<RaftRequest>, StorageError<NodeId>> {
        self.log_storage
            .sync()
            .await
            .map_err(|e| StorageError::write(&e))?;
        Ok(LogFlushed::new(None, None))
    }
}

#[async_trait]
impl<L, S, A> RaftStateMachine<RaftRequest, RaftResponse>
    for OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
    type SnapshotBuilder = Self;

    async fn applied_state(
        &mut self,
    ) -> Result<
        (Option<LogId<NodeId>>, openraft::StoredMembership<NodeId>),
        StorageError<NodeId>,
    > {
        let last_applied = *self.last_applied.read().await;
        let membership = self.last_membership.read().await.clone();
        Ok((last_applied, membership))
    }

    async fn apply<I>(
        &mut self,
        entries: I,
    ) -> Result<Vec<RaftResponse>, StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<RaftRequest>> + Send,
    {
        let mut responses = Vec::new();
        let mut last_applied = self.last_applied.write().await;
        let mut last_membership = self.last_membership.write().await;

        for entry in entries {
            *last_applied = Some(*entry.get_log_id());

            match entry.payload {
                EntryPayload::Blank => {
                    responses.push(RaftResponse {
                        success: true,
                        message: "Blank entry applied".to_string(),
                        data: None,
                    });
                }
                EntryPayload::Normal(request) => {
                    match self.apply_request(&request).await {
                        Ok(response) => responses.push(response),
                        Err(e) => responses.push(RaftResponse {
                            success: false,
                            message: e.to_string(),
                            data: None,
                        }),
                    }
                }
                EntryPayload::Membership(membership) => {
                    *last_membership = membership;
                    responses.push(RaftResponse {
                        success: true,
                        message: "Membership change applied".to_string(),
                        data: None,
                    });
                }
            }
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        Self {
            log_storage: self.log_storage.clone(),
            vote: self.vote.clone(),
            snapshot_storage: self.snapshot_storage.clone(),
            snapshot_meta: self.snapshot_meta.clone(),
            app_storage: self.app_storage.clone(),
            last_applied: self.last_applied.clone(),
            last_membership: self.last_membership.clone(),
        }
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<NodeId, RaftNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<NodeId>> {
        let data = snapshot.into_inner();

        // Deserialize and install the application data
        let app_data: Vec<(StorageValue, StorageValue)> =
            bincode::deserialize(&data).map_err(|e| StorageError::read(&e))?;

        self.app_storage
            .import_all(app_data)
            .await
            .map_err(|e| StorageError::write(&e))?;

        *self.last_applied.write().await = Some(meta.last_log_id);
        *self.last_membership.write().await = meta.last_membership.clone();

        // Store snapshot metadata
        *self.snapshot_meta.write().await = Some(meta.clone());

        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<
        Option<Snapshot<NodeId, RaftNode, Cursor<Vec<u8>>>>,
        StorageError<NodeId>,
    > {
        let snapshot_meta = self.snapshot_meta.read().await;

        if let Some(meta) = snapshot_meta.as_ref() {
            // Build current snapshot
            let snapshot = self.build_application_snapshot().await?;
            let cursor = Cursor::new(snapshot.data);

            Ok(Some(Snapshot {
                meta: meta.clone(),
                snapshot: Box::new(cursor),
            }))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl<L, S, A> RaftSnapshotBuilder<RaftRequest, Cursor<Vec<u8>>>
    for OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
    async fn build_snapshot(
        &mut self,
    ) -> Result<Snapshot<NodeId, RaftNode, Cursor<Vec<u8>>>, StorageError<NodeId>>
    {
        let snapshot = self.build_application_snapshot().await?;
        let last_applied = self.last_applied.read().await;
        let last_membership = self.last_membership.read().await;

        let meta = SnapshotMeta {
            last_log_id: last_applied.unwrap_or_default(),
            last_membership: last_membership.clone(),
            snapshot_id: format!(
                "snapshot-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            ),
        };

        let cursor = Cursor::new(snapshot.data);

        // Store the snapshot using our snapshot storage
        let internal_snapshot = oprc_dp_storage::RaftSnapshot {
            id: meta.snapshot_id.clone(),
            last_included_index: meta.last_log_id.index,
            last_included_term: meta.last_log_id.leader_id.term,
            membership_config: bincode::serialize(&meta.last_membership)
                .unwrap_or_default(),
            data: cursor.get_ref().clone(),
            metadata: oprc_dp_storage::SnapshotMetadata {
                id: meta.snapshot_id.clone(),
                last_included_index: meta.last_log_id.index,
                last_included_term: meta.last_log_id.leader_id.term,
                size_bytes: cursor.get_ref().len() as u64,
                compressed_size_bytes: cursor.get_ref().len() as u64, // Will be compressed by storage
                created_at: std::time::SystemTime::now(),
                compression: oprc_dp_storage::CompressionType::None,
                checksum: "".to_string(), // Will be calculated by storage
                entry_count: 0,           // Will be calculated by storage
            },
        };

        self.snapshot_storage
            .create_snapshot(internal_snapshot)
            .await
            .map_err(|e| StorageError::write(&e))?;

        // Store metadata for future retrieval
        *self.snapshot_meta.write().await = Some(meta.clone());

        Ok(Snapshot {
            meta,
            snapshot: Box::new(cursor),
        })
    }
}

impl<L, S, A> RaftStorage<RaftRequest, RaftResponse>
    for OpenRaftStorage<L, S, A>
where
    L: RaftLogStorageTrait + Clone + 'static,
    S: RaftSnapshotStorage + Clone + 'static,
    A: ApplicationDataStorage + Clone + 'static,
{
}

/// Type alias for memory-based Raft storage (useful for testing)
pub type MemoryRaftStorage = OpenRaftStorage<
    AppendOnlyLogStorage<MemoryStorage>,
    CompressedSnapshotStorage<MemoryStorage>,
    EnhancedApplicationStorage<MemoryStorage>,
>;
