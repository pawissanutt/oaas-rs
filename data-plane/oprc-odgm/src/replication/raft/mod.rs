use crate::replication::{ReplicationResponse, ShardRequest};

// OpenRaft implementation modules
mod log;
pub mod raft_layer;
mod raft_network;
mod state_machine;

pub use log::OpenraftLogStore;
pub use state_machine::ObjectShardStateMachine;

// pub use raft_layer::{
//     create_memory_raft_replication_layer, OpenRaftReplicationLayer,
// };
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
