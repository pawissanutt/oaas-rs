use crate::replication::{ReplicationResponse, ShardRequest};

// OpenRaft implementation modules
// pub mod raft_layer;
mod state_machine;

// pub use raft_layer::{
//     create_memory_raft_replication_layer, OpenRaftReplicationLayer,
// };

openraft::declare_raft_types!(
    pub ReplicationTypeConfig:
        D = ShardRequest,
        R = ReplicationResponse,
        NodeId = u64,
        Node = openraft::BasicNode,
        Entry = openraft::Entry<ReplicationTypeConfig>,
        SnapshotData = std::io::Cursor<Vec<u8>>,
        AsyncRuntime = openraft::TokioRuntime,
);
