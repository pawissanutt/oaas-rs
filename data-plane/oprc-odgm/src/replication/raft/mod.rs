use crate::replication::{ReplicationResponse, ShardRequest};

// OpenRaft implementation modules
mod raft_layer;
mod raft_log;
mod raft_network;
mod state_machine;

pub use raft_log::OpenraftLogStore;
pub use state_machine::ObjectShardStateMachine;
type NodeId = u64;

openraft::declare_raft_types!(
    pub ReplicationTypeConfig:
        D = ShardRequest,
        R = ReplicationResponse,
        NodeId = NodeId,
        Node = openraft::BasicNode,
        Entry = openraft::Entry<ReplicationTypeConfig>,
        SnapshotData = std::io::Cursor<Vec<u8>>,
        AsyncRuntime = openraft::TokioRuntime,
);
