use std::io::Cursor;

use openraft::{
    storage::RaftStateMachine, Entry, ErrorSubject, ErrorVerb, LeaderId, LogId,
    RaftSnapshotBuilder, RaftTypeConfig, Snapshot, SnapshotMeta, StorageError,
    StoredMembership,
};
use oprc_dp_storage::{
    KvStreamingRaftSnapshot, RaftLogId, RaftLogStorage, RaftMembership,
    RaftSnapshotMeta, SnapshotCapableStorage, SnapshotCapableStorageExt,
};
use tokio::io::AsyncReadExt;

use crate::replication::{ReplicationResponse, ShardRequest};

#[derive(Default, Clone)]
pub struct ObjectShardStateMachine<L, A>
where
    A: SnapshotCapableStorage + Default + Clone + 'static,
    L: RaftLogStorage + Default + Clone + 'static,
{
    log: L,
    app: A,
}

impl<L, A, C> RaftSnapshotBuilder<C> for ObjectShardStateMachine<L, A>
where
    A: SnapshotCapableStorage + Default + Clone + 'static,
    L: RaftLogStorage + Default + Clone + 'static,
    C: RaftTypeConfig<
        SnapshotData = Cursor<Vec<u8>>,
        Entry = Entry<C>,
        NodeId = u64,
    >,
{
    #[tracing::instrument(level = "debug", skip(self))]
    async fn build_snapshot(
        &mut self,
    ) -> Result<Snapshot<C>, StorageError<C::NodeId>> {
        // Get current Raft state from log storage - NOW THIS WORKS!
        let (log_id, membership) = self
            .log
            .applied_state()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Create zero-copy snapshot first
        let zero_copy_snapshot = self
            .app
            .create_zero_copy_snapshot()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Create key-value streaming Raft snapshot
        let kv_raft_snapshot = KvStreamingRaftSnapshot {
            zero_copy_snapshot,
            log_id: log_id
                .as_ref()
                .map(|id| format!("{}:{}", id.term, id.index)),
            membership: format!(
                "config:{} voters:{:?}",
                membership.config_id, membership.voters
            ),
            snapshot_id: format!("snapshot-{}", chrono::Utc::now().timestamp()),
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
            last_log_id: log_id
                .map(|id| LogId::new(LeaderId::new(id.term, 0), id.index)),
            last_membership: convert_to_openraft_membership::<C::Node>(
                &membership,
            ),
            snapshot_id: kv_raft_snapshot.snapshot_id.clone(),
        };

        Ok(Snapshot {
            meta: snapshot_meta,
            snapshot: Box::new(Cursor::new(buffer)),
        })
    }
}

impl<A, L, C> RaftStateMachine<C> for ObjectShardStateMachine<L, A>
where
    A: SnapshotCapableStorage + Default + Clone + 'static,
    L: RaftLogStorage + Default + Clone + 'static,
    C: RaftTypeConfig<
        SnapshotData = Cursor<Vec<u8>>,
        Entry = Entry<C>,
        NodeId = u64,
    >,
{
    type SnapshotBuilder = Self;
    #[tracing::instrument(level = "debug", skip(self))]
    async fn applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<C::NodeId>>,
            StoredMembership<C::NodeId, C::Node>,
        ),
        StorageError<C::NodeId>,
    > {
        // Get state from log storage and convert to OpenRaft types
        let (log_id, membership) = self
            .log
            .applied_state()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        let openraft_log_id =
            log_id.map(|id| LogId::new(LeaderId::new(id.term, 0), id.index));
        let openraft_membership =
            convert_to_openraft_membership::<C::Node>(&membership);

        Ok((openraft_log_id, openraft_membership))
    }

    #[tracing::instrument(level = "debug", skip(self, _entries))]
    async fn apply<I>(
        &mut self,
        _entries: I,
    ) -> Result<Vec<C::R>, StorageError<C::NodeId>>
    where
        I: IntoIterator<Item = C::Entry> + Send,
    {
        // TODO: Implement proper entry application logic
        // This is not related to snapshot functionality
        todo!("Entry application logic not implemented yet")
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<C::SnapshotData>, StorageError<C::NodeId>> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    #[tracing::instrument(level = "debug", skip(self, snapshot))]
    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<C::NodeId, C::Node>,
        snapshot: Box<C::SnapshotData>,
    ) -> Result<(), StorageError<C::NodeId>> {
        // Create cursor reader from snapshot data
        let snapshot_len = snapshot.get_ref().len() as u64;
        let cursor_reader = Cursor::new(snapshot.into_inner());

        // Install snapshot through key-value streaming interface
        self.app
            .install_kv_snapshot_from_cursor(cursor_reader)
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Update log storage state
        let raft_snapshot_meta = RaftSnapshotMeta {
            last_log_id: meta.last_log_id.map(|id| RaftLogId {
                term: id.leader_id.term,
                index: id.index,
            }),
            last_membership: convert_from_openraft_membership::<C::Node>(
                &meta.last_membership,
            ),
            snapshot_id: meta.snapshot_id.clone(),
            created_at: std::time::SystemTime::now(),
            size_bytes: snapshot_len,
            checksum: None,
        };

        self.log
            .install_snapshot_state(&raft_snapshot_meta)
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<C>>, StorageError<C::NodeId>> {
        // Check if we have a recent snapshot
        if let Some(zero_copy_snapshot) = self
            .app
            .latest_snapshot()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?
        {
            // Get current Raft state
            let (log_id, membership) = self
                .log
                .applied_state()
                .await
                .map_err(|e| Self::storage_error(&e.to_string()))?;

            // Create key-value streaming Raft-compatible snapshot
            let kv_raft_snapshot = KvStreamingRaftSnapshot {
                zero_copy_snapshot,
                log_id: log_id
                    .as_ref()
                    .map(|id| format!("{}:{}", id.term, id.index)),
                membership: format!(
                    "config:{} voters:{:?}",
                    membership.config_id, membership.voters
                ),
                snapshot_id: format!(
                    "snapshot-{}",
                    chrono::Utc::now().timestamp()
                ),
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
                last_log_id: log_id
                    .map(|id| LogId::new(LeaderId::new(id.term, 0), id.index)),
                last_membership: convert_to_openraft_membership::<C::Node>(
                    &membership,
                ),
                snapshot_id: kv_raft_snapshot.snapshot_id.clone(),
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

impl<L, A> ObjectShardStateMachine<L, A>
where
    A: SnapshotCapableStorage + Default + Clone + 'static,
    L: RaftLogStorage + Default + Clone + 'static,
{
    /// Convert storage error to OpenRaft storage error
    fn storage_error(msg: &str) -> StorageError<u64> {
        StorageError::from_io_error(
            ErrorSubject::Store,
            ErrorVerb::Read,
            std::io::Error::new(std::io::ErrorKind::Other, msg),
        )
    }

    /// Apply a shard request to the application storage
    async fn apply_request<C>(
        &mut self,
        _request: ShardRequest,
    ) -> Result<ReplicationResponse, StorageError<C::NodeId>>
    where
        C: RaftTypeConfig<NodeId = u64>,
    {
        // TODO: Implement shard request handling
        // This is not related to snapshot functionality
        todo!("Shard request handling not implemented yet")
    }
}

/// Convert our RaftMembership to OpenRaft's StoredMembership
fn convert_to_openraft_membership<N>(
    membership: &RaftMembership,
) -> StoredMembership<u64, N>
where
    N: openraft::Node,
{
    use std::collections::BTreeSet;

    let voters: BTreeSet<u64> = membership.voters.clone();
    let learners: BTreeSet<u64> = membership.learners.clone();

    // Create a basic membership without joint consensus
    let basic_membership =
        openraft::Membership::new(vec![voters], Some(learners));

    let leader_id = LeaderId::new(0, 0); // Default term and leader
    StoredMembership::new(
        Some(LogId::new(leader_id, membership.config_id)),
        basic_membership,
    )
}

/// Convert OpenRaft's StoredMembership to our RaftMembership  
fn convert_from_openraft_membership<N>(
    membership: &StoredMembership<u64, N>,
) -> RaftMembership
where
    N: openraft::Node,
{
    // Get the first config from joint membership
    let configs = membership.membership().get_joint_config();
    let config = configs.first().cloned().unwrap_or_default();

    RaftMembership {
        config_id: membership.log_id().map(|id| id.index).unwrap_or(0),
        voters: config,
        learners: membership.membership().learner_ids().collect(),
    }
}
