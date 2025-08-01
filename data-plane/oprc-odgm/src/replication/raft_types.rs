use std::collections::BTreeMap;
use std::time::SystemTime;

use openraft::Config;
use serde::{Deserialize, Serialize};

use crate::replication::ShardRequest;

/// Node ID type for Raft
pub type NodeId = u64;

/// Raft log entry index type  
pub type LogIndex = u64;

/// Raft term type
pub type Term = u64;

/// Application data for Raft state machine
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RaftRequest {
    pub shard_request: ShardRequest,
}

/// Application response from Raft state machine
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RaftResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<Vec<u8>>,
}

/// Snapshot data for Raft
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct RaftSnapshot {
    pub meta: RaftSnapshotMeta,
    /// The data of the state machine at the time of this snapshot.
    pub data: Vec<u8>,
}

/// Snapshot metadata
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RaftSnapshotMeta {
    /// Applied log index when this snapshot was created
    pub last_applied: LogIndex,

    /// Applied log term when this snapshot was created  
    pub last_applied_term: Term,

    /// Current membership configuration
    pub last_membership: openraft::StoredMembership<NodeId, RaftNode>,

    /// Timestamp when snapshot was created
    pub created_at: SystemTime,

    /// Size of snapshot data in bytes
    pub size_bytes: u64,
}

impl Default for RaftSnapshotMeta {
    fn default() -> Self {
        Self {
            last_applied: 0,
            last_applied_term: 0,
            last_membership: openraft::StoredMembership::default(),
            created_at: SystemTime::UNIX_EPOCH,
            size_bytes: 0,
        }
    }
}

/// Create default Raft configuration
pub fn create_raft_config() -> Config {
    Config {
        heartbeat_interval: 1000,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        ..Default::default()
    }
}

/// Network address of a node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeAddress {
    pub host: String,
    pub port: u16,
}

impl NodeAddress {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub fn to_string(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Raft node information  
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftNode {
    pub node_id: NodeId,
    pub address: NodeAddress,
    pub is_learner: bool,
}

impl RaftNode {
    pub fn new(node_id: NodeId, address: NodeAddress) -> Self {
        Self {
            node_id,
            address,
            is_learner: false,
        }
    }

    pub fn learner(node_id: NodeId, address: NodeAddress) -> Self {
        Self {
            node_id,
            address,
            is_learner: true,
        }
    }
}

impl Default for RaftNode {
    fn default() -> Self {
        Self {
            node_id: 0,
            address: NodeAddress::new("localhost", 8080),
            is_learner: false,
        }
    }
}

/// Cluster membership configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMembership {
    pub nodes: BTreeMap<NodeId, RaftNode>,
    pub learners: BTreeMap<NodeId, RaftNode>,
}

impl ClusterMembership {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            learners: BTreeMap::new(),
        }
    }

    pub fn add_node(&mut self, node: RaftNode) {
        if node.is_learner {
            self.learners.insert(node.node_id, node);
        } else {
            self.nodes.insert(node.node_id, node);
        }
    }

    pub fn remove_node(&mut self, node_id: NodeId) {
        self.nodes.remove(&node_id);
        self.learners.remove(&node_id);
    }

    pub fn get_node(&self, node_id: NodeId) -> Option<&RaftNode> {
        self.nodes
            .get(&node_id)
            .or_else(|| self.learners.get(&node_id))
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.learners.len()
    }

    pub fn voting_node_count(&self) -> usize {
        self.nodes.len()
    }
}

impl Default for ClusterMembership {
    fn default() -> Self {
        Self::new()
    }
}
