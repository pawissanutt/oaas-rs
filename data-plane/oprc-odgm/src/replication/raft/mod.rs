use crate::replication::{ReplicationResponse, ShardRequest};

// OpenRaft implementation modules
mod raft_layer;
mod raft_log;
mod raft_network;
mod snapshot_integration;
mod state_machine;
mod streaming_snapshot;

pub use raft_layer::OpenRaftReplicationLayer;
pub use raft_log::OpenraftLogStore;
pub use snapshot_integration::{
    create_raft_snapshot, create_raft_snapshot_from_existing,
    install_raft_snapshot,
};
pub use state_machine::ObjectShardStateMachine;
pub use streaming_snapshot::StreamingSnapshotBuffer;

type NodeId = u64;

openraft::declare_raft_types!(
    pub ReplicationTypeConfig:
        D = ShardRequest,
        R = ReplicationResponse,
        NodeId = NodeId,
        Node = openraft::BasicNode,
        Entry = openraft::Entry<ReplicationTypeConfig>,
        SnapshotData = StreamingSnapshotBuffer,
        AsyncRuntime = openraft::TokioRuntime,
);
