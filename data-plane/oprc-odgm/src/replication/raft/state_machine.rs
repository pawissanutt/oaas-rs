use openraft::{
    storage::RaftStateMachine, BasicNode, Entry, LogId, RaftSnapshotBuilder,
    Snapshot, SnapshotMeta, StorageError, StorageIOError, StoredMembership,
};
use oprc_dp_storage::SnapshotCapableStorage;

use crate::replication::raft::{
    create_raft_snapshot, create_raft_snapshot_from_existing,
    install_raft_snapshot, ReplicationTypeConfig, StreamingSnapshotBuffer,
};
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

    /// Current snapshot maintained by this state machine
    current_snapshot: Option<oprc_dp_storage::Snapshot<A::SnapshotData>>,
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
            current_snapshot: None,
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
                let out = self
                    .app
                    .put(write_operation.key.as_slice(), write_operation.value)
                    .await
                    .map_err(|e| Self::storage_error(&e.to_string()))?;
                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    extra: crate::replication::OperationExtra::Write(out),
                    ..Default::default()
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
                    ..Default::default()
                })
            }
            crate::replication::Operation::Delete(delete_operation) => {
                self.app
                    .delete(delete_operation.key.as_slice())
                    .await
                    .map_err(|e| Self::storage_error(&e.to_string()))?;
                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    ..Default::default()
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
        // Create storage snapshot and track it
        let storage_snapshot = self
            .app
            .create_snapshot()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Update our current snapshot tracking
        self.current_snapshot = Some(storage_snapshot.clone());

        create_raft_snapshot(
            &self.app,
            self.last_applied_log_id,
            self.last_membership.clone(),
            self.snapshot_idx,
        )
        .await
        .map_err(|e| Self::storage_error(&e.to_string()))
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
        let mut state_changed = false;

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
                    state_changed = true;
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
                        ..Default::default()
                    });
                    state_changed = true;
                }
            }
        }

        // If state changed, invalidate current snapshot as it's no longer current
        if state_changed {
            self.current_snapshot = None;
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
    ) -> Result<Box<StreamingSnapshotBuffer>, StorageError<u64>> {
        Ok(Box::new(StreamingSnapshotBuffer::new()))
    }

    #[tracing::instrument(level = "debug", skip(self, snapshot))]
    async fn install_snapshot(
        &mut self,
        meta: &SnapshotMeta<u64, BasicNode>,
        snapshot: Box<StreamingSnapshotBuffer>,
    ) -> Result<(), StorageError<u64>> {
        // Install snapshot directly
        install_raft_snapshot(&self.app, *snapshot)
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

        // Update state machine's tracking
        self.last_applied_log_id = meta.last_log_id;
        self.last_membership = meta.last_membership.clone();

        // Create a new snapshot after installation to track current state
        let new_snapshot = self
            .app
            .create_snapshot()
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;
        self.current_snapshot = Some(new_snapshot);

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<ReplicationTypeConfig>>, StorageError<u64>>
    {
        // Check if we have a current snapshot tracked by this state machine
        if let Some(ref current_snapshot) = self.current_snapshot {
            // Create Raft snapshot from our tracked snapshot
            let raft_snapshot = create_raft_snapshot_from_existing(
                &self.app,
                current_snapshot,
                self.last_applied_log_id,
                self.last_membership.clone(),
                self.snapshot_idx,
            )
            .await
            .map_err(|e| Self::storage_error(&e.to_string()))?;

            Ok(Some(raft_snapshot))
        } else {
            Ok(None)
        }
    }
}
