use std::collections::HashMap;
use std::io::Cursor;

use openraft::{
    storage::RaftStateMachine, BasicNode, Entry, LogId, RaftSnapshotBuilder,
    Snapshot, SnapshotMeta, StorageError, StorageIOError, StoredMembership,
};
use oprc_dp_storage::{
    KvStreamingRaftSnapshot, SnapshotCapableStorage, SnapshotCapableStorageExt,
};
use tokio::io::AsyncReadExt;

use crate::replication::raft::ReplicationTypeConfig;
use crate::replication::{ReplicationResponse, ResponseStatus, ShardRequest};

/// ObjectShardStateMachine only handles application state
/// OpenRaft handles log storage separately through RaftLogStorage
#[derive(Debug, Clone)]
pub struct ObjectShardStateMachine<A>
where
    A: SnapshotCapableStorage + Clone + 'static,
{
    /// Application storage backend
    app: A,

    /// Last applied log ID (maintained by state machine)
    last_applied_log_id: Option<LogId<u64>>,

    /// Last membership configuration (maintained by state machine)
    last_membership: StoredMembership<u64, BasicNode>,

    /// Snapshot index counter for unique snapshot IDs
    snapshot_idx: u64,
}

impl<A> ObjectShardStateMachine<A>
where
    A: SnapshotCapableStorage + Clone + 'static,
{
    /// Create a new state machine with the given application storage
    pub fn new(app: A) -> Self {
        Self {
            app,
            last_applied_log_id: None,
            last_membership: StoredMembership::default(),
            snapshot_idx: 0,
        }
    }

    /// Create state machine with specific application storage
    pub fn with_app(app: A) -> Self {
        Self::new(app)
    }

    /// Apply a shard request to the application storage
    async fn apply_shard_request(
        &mut self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        match request.operation {
            crate::replication::Operation::Write(write_operation) => {
                self.app
                    .put(write_operation.key.as_slice(), write_operation.value)
                    .await
                    .map_err(|e| Self::storage_error(&e.to_string()))?;
                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: None,
                    metadata: HashMap::new(),
                })
            }
            crate::replication::Operation::Read(read_operation) => {
                let val = self
                    .app
                    .get(read_operation.key.as_slice())
                    .await
                    .map_err(|e| Self::storage_error(&e.to_string()))?;
                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: val,
                    metadata: HashMap::new(),
                })
            }
            crate::replication::Operation::Delete(delete_operation) => {
                self.app
                    .delete(delete_operation.key.as_slice())
                    .await
                    .map_err(|e| Self::storage_error(&e.to_string()))?;
                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: None,
                    metadata: HashMap::new(),
                })
            }
            crate::replication::Operation::Batch(_operations) => {
                todo!()
            }
        }
    }

    /// Helper method to create storage errors
    fn storage_error(msg: &str) -> StorageError<u64> {
        StorageError::IO {
            source: StorageIOError::read(&std::io::Error::new(
                std::io::ErrorKind::Other,
                msg,
            )),
        }
    }
}

impl<A> RaftSnapshotBuilder<ReplicationTypeConfig>
    for ObjectShardStateMachine<A>
where
    A: SnapshotCapableStorage + Clone + 'static,
{
    #[tracing::instrument(level = "debug", skip(self))]
    async fn build_snapshot(
        &mut self,
    ) -> Result<Snapshot<ReplicationTypeConfig>, StorageError<u64>> {
        // Create zero-copy snapshot from application storage
        let zero_copy_snapshot = self
            .app
            .create_zero_copy_snapshot()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Create unique snapshot ID
        let snapshot_id = if let Some(last) = self.last_applied_log_id {
            format!("{}-{}-{}", last.leader_id, last.index, self.snapshot_idx)
        } else {
            format!("--{}", self.snapshot_idx)
        };

        // Create key-value streaming Raft snapshot
        let kv_raft_snapshot = KvStreamingRaftSnapshot {
            zero_copy_snapshot,
            log_id: self
                .last_applied_log_id
                .as_ref()
                .map(|id| format!("{}:{}", id.leader_id.term, id.index)),
            membership: format!(
                "voters:{:?} learners:{:?}",
                self.last_membership
                    .membership()
                    .voter_ids()
                    .collect::<Vec<_>>(),
                self.last_membership
                    .membership()
                    .learner_ids()
                    .collect::<Vec<_>>()
            ),
            snapshot_id: snapshot_id.clone(),
            format_version: 1,
        };

        // Create streaming reader for key-value pairs
        let mut kv_stream_reader = kv_raft_snapshot
            .create_kv_stream_reader(&self.app)
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Stream key-value data into memory buffer (chunked to avoid large allocations)
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 8192]; // 8KB chunks

        loop {
            match kv_stream_reader.read(&mut chunk).await {
                Ok(0) => break, // EOF
                Ok(bytes_read) => {
                    buffer.extend_from_slice(&chunk[..bytes_read]);

                    // Optional: Add backpressure control for very large snapshots
                    if buffer.len() > 100 * 1024 * 1024 {
                        // 100MB limit
                        return Err(Self::storage_error(
                            "Snapshot too large for memory",
                        ));
                    }
                }
                Err(e) => return Err(Self::storage_error(&e.to_string())),
            }
        }

        // Create OpenRaft snapshot with proper metadata
        let snapshot_meta = SnapshotMeta {
            last_log_id: self.last_applied_log_id,
            last_membership: self.last_membership.clone(),
            snapshot_id,
        };

        Ok(Snapshot {
            meta: snapshot_meta,
            snapshot: Box::new(Cursor::new(buffer)),
        })
    }
}

impl<A> RaftStateMachine<ReplicationTypeConfig> for ObjectShardStateMachine<A>
where
    A: SnapshotCapableStorage + Clone + 'static,
{
    type SnapshotBuilder = Self;

    #[tracing::instrument(level = "debug", skip(self))]
    async fn applied_state(
        &mut self,
    ) -> Result<
        (Option<LogId<u64>>, StoredMembership<u64, BasicNode>),
        StorageError<u64>,
    > {
        // Return the state maintained by this state machine
        // OpenRaft will coordinate with the log storage separately
        Ok((self.last_applied_log_id, self.last_membership.clone()))
    }

    #[tracing::instrument(level = "debug", skip(self, entries))]
    async fn apply<I>(
        &mut self,
        entries: I,
    ) -> Result<Vec<ReplicationResponse>, StorageError<u64>>
    where
        I: IntoIterator<Item = Entry<ReplicationTypeConfig>> + Send,
    {
        let entries = entries.into_iter();
        let mut responses = Vec::with_capacity(entries.size_hint().0);

        for entry in entries {
            // Update our tracking of the last applied log
            self.last_applied_log_id = Some(entry.log_id);

            // Process the entry based on its payload
            match &entry.payload {
                // Blank entries (heartbeats, etc.)
                openraft::EntryPayload::Blank => {
                    // No-op, just update log tracking
                }

                // Normal application entries
                openraft::EntryPayload::Normal(request) => {
                    // Apply the request to application storage
                    let response = self
                        .apply_shard_request(request.clone())
                        .await
                        .map_err(|e| Self::storage_error(&e.to_string()))?;
                    responses.push(response);
                }

                // Membership change entries
                openraft::EntryPayload::Membership(membership) => {
                    // Update our membership state
                    self.last_membership = StoredMembership::new(
                        Some(entry.log_id),
                        membership.clone(),
                    );

                    // For membership changes, we might not have a specific response
                    // This depends on your ShardRequest/ReplicationResponse types
                    // For now, we'll create a default response
                    responses.push(ReplicationResponse {
                        status: ResponseStatus::Applied,
                        data: None,
                        metadata: Default::default(),
                    });
                }
            }
        }

        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        // Increment snapshot index for unique IDs
        self.snapshot_idx += 1;
        self.clone()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<Cursor<Vec<u8>>>, StorageError<u64>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    #[tracing::instrument(level = "debug", skip(self, snapshot))]
    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<u64, BasicNode>,
        snapshot: Box<Cursor<Vec<u8>>>,
    ) -> Result<(), StorageError<u64>> {
        // Create cursor reader from snapshot data
        let cursor_reader = Cursor::new(snapshot.into_inner());

        // Install snapshot through key-value streaming interface
        self.app
            .install_kv_snapshot_from_cursor(cursor_reader)
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Update state machine's tracking
        self.last_applied_log_id = meta.last_log_id;
        self.last_membership = meta.last_membership.clone();

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<ReplicationTypeConfig>>, StorageError<u64>>
    {
        // Check if we have a recent snapshot
        if let Some(zero_copy_snapshot) = self
            .app
            .latest_snapshot()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?
        {
            // Create unique snapshot ID
            let snapshot_id = if let Some(last) = self.last_applied_log_id {
                format!(
                    "{}-{}-{}",
                    last.leader_id, last.index, self.snapshot_idx
                )
            } else {
                format!("--{}", self.snapshot_idx)
            };

            // Create key-value streaming Raft-compatible snapshot
            let kv_raft_snapshot = KvStreamingRaftSnapshot {
                zero_copy_snapshot,
                log_id: self
                    .last_applied_log_id
                    .as_ref()
                    .map(|id| format!("{}:{}", id.leader_id.term, id.index)),
                membership: format!(
                    "voters:{:?} learners:{:?}",
                    self.last_membership
                        .membership()
                        .voter_ids()
                        .collect::<Vec<_>>(),
                    self.last_membership
                        .membership()
                        .learner_ids()
                        .collect::<Vec<_>>()
                ),
                snapshot_id: snapshot_id.clone(),
                format_version: 1,
            };

            // Stream key-value pairs to buffer (chunked reading)
            let mut kv_stream_reader = kv_raft_snapshot
                .create_kv_stream_reader(&self.app)
                .await
                .map_err(|e| Self::storage_error(&e.to_string()))?;

            let mut buffer = Vec::new();
            let mut chunk = [0u8; 8192]; // 8KB chunks

            loop {
                match kv_stream_reader.read(&mut chunk).await {
                    Ok(0) => break, // EOF
                    Ok(bytes_read) => {
                        buffer.extend_from_slice(&chunk[..bytes_read]);

                        // Optional: Add backpressure control for very large snapshots
                        if buffer.len() > 100 * 1024 * 1024 {
                            // 100MB limit
                            return Err(Self::storage_error(
                                "Snapshot too large for memory",
                            ));
                        }
                    }
                    Err(e) => return Err(Self::storage_error(&e.to_string())),
                }
            }

            let snapshot_meta = SnapshotMeta {
                last_log_id: self.last_applied_log_id,
                last_membership: self.last_membership.clone(),
                snapshot_id,
            };

            Ok(Some(Snapshot {
                meta: snapshot_meta,
                snapshot: Box::new(Cursor::new(buffer)),
            }))
        } else {
            Ok(None)
        }
    }
}
