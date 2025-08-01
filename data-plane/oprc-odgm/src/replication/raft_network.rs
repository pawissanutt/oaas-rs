use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use openraft::error::{NetworkError, RPCError, RemoteError, Unreachable};
use openraft::{RaftNetwork, RaftNetworkFactory};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::raft_types::*;
use crate::replication::ReplicationError;

/// Network implementation for OpenRaft
#[derive(Debug, Clone)]
pub struct OpenRaftNetwork {
    /// Mapping of node IDs to their network addresses
    nodes: Arc<RwLock<HashMap<NodeId, NodeAddress>>>,
    
    /// HTTP client for making network requests
    client: reqwest::Client,
}

impl OpenRaftNetwork {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            client: reqwest::Client::new(),
        }
    }

    pub async fn add_node(&self, node_id: NodeId, address: NodeAddress) {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node_id, address);
    }

    pub async fn remove_node(&self, node_id: NodeId) {
        let mut nodes = self.nodes.write().await;
        nodes.remove(&node_id);
    }

    pub async fn get_node_address(&self, node_id: NodeId) -> Option<NodeAddress> {
        let nodes = self.nodes.read().await;
        nodes.get(&node_id).cloned()
    }

    /// Build URL for a node's endpoint
    fn build_url(&self, address: &NodeAddress, endpoint: &str) -> String {
        format!("http://{}:{}/{}", address.host, address.port, endpoint)
    }
}

/// Request types for Raft network communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    pub vote: openraft::Vote<NodeId>,
    pub prev_log_id: Option<openraft::LogId<NodeId>>,
    pub entries: Vec<openraft::Entry<RaftRequest>>,
    pub leader_commit: Option<openraft::LogId<NodeId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    pub vote: openraft::Vote<NodeId>,
    pub success: bool,
    pub conflict: Option<openraft::LogId<NodeId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    pub vote: openraft::Vote<NodeId>,
    pub last_log_id: Option<openraft::LogId<NodeId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteResponse {
    pub vote: openraft::Vote<NodeId>,
    pub vote_granted: bool,
    pub last_log_id: Option<openraft::LogId<NodeId>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallSnapshotRequest {
    pub vote: openraft::Vote<NodeId>,
    pub meta: openraft::SnapshotMeta<NodeId, RaftNode>,
    pub snapshot: Box<RaftSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallSnapshotResponse {
    pub vote: openraft::Vote<NodeId>,
}

#[async_trait]
impl RaftNetwork<RaftRequest> for OpenRaftNetwork {
    async fn append_entries(
        &mut self,
        target: NodeId,
        rpc: openraft::raft::AppendEntriesRequest<RaftRequest>,
    ) -> Result<
        openraft::raft::AppendEntriesResponse<NodeId>,
        RPCError<NodeId, RaftNode, openraft::error::AppendEntriesError<NodeId>>,
    > {
        let address = self.get_node_address(target).await.ok_or_else(|| {
            RPCError::Network(NetworkError::new(&Unreachable::new(&format!(
                "Node {} not found in network",
                target
            ))))
        })?;

        let url = self.build_url(&address, "raft/append_entries");
        
        let request = AppendEntriesRequest {
            vote: rpc.vote,
            prev_log_id: rpc.prev_log_id,
            entries: rpc.entries,
            leader_commit: rpc.leader_commit,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                RPCError::Network(NetworkError::new(&Unreachable::new(&e.to_string())))
            })?;

        if !response.status().is_success() {
            return Err(RPCError::Network(NetworkError::new(&Unreachable::new(
                &format!("HTTP error: {}", response.status()),
            ))));
        }

        let append_response: AppendEntriesResponse = response.json().await.map_err(|e| {
            RPCError::Network(NetworkError::new(&Unreachable::new(&e.to_string())))
        })?;

        Ok(openraft::raft::AppendEntriesResponse {
            vote: append_response.vote,
            success: append_response.success,
            conflict: append_response.conflict,
        })
    }

    async fn install_snapshot(
        &mut self,
        target: NodeId,
        rpc: openraft::raft::InstallSnapshotRequest<NodeId, RaftNode>,
    ) -> Result<
        openraft::raft::InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, RaftNode, openraft::error::InstallSnapshotError>,
    > {
        let address = self.get_node_address(target).await.ok_or_else(|| {
            RPCError::Network(NetworkError::new(&Unreachable::new(&format!(
                "Node {} not found in network",
                target
            ))))
        })?;

        let url = self.build_url(&address, "raft/install_snapshot");
        
        // Convert snapshot data - this is a simplified approach
        let snapshot_data = RaftSnapshot {
            meta: RaftSnapshotMeta {
                last_applied: rpc.meta.last_log_id.map(|id| id.index).unwrap_or(0),
                last_applied_term: rpc.meta.last_log_id.map(|id| id.leader_id).unwrap_or(0),
                last_membership: rpc.meta.last_membership.clone(),
                created_at: std::time::SystemTime::now(),
                size_bytes: rpc.snapshot.len() as u64,
            },
            data: rpc.snapshot,
        };
        
        let request = InstallSnapshotRequest {
            vote: rpc.vote,
            meta: rpc.meta,
            snapshot: Box::new(snapshot_data),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                RPCError::Network(NetworkError::new(&Unreachable::new(&e.to_string())))
            })?;

        if !response.status().is_success() {
            return Err(RPCError::Network(NetworkError::new(&Unreachable::new(
                &format!("HTTP error: {}", response.status()),
            ))));
        }

        let install_response: InstallSnapshotResponse = response.json().await.map_err(|e| {
            RPCError::Network(NetworkError::new(&Unreachable::new(&e.to_string())))
        })?;

        Ok(openraft::raft::InstallSnapshotResponse {
            vote: install_response.vote,
        })
    }

    async fn vote(
        &mut self,
        target: NodeId,
        rpc: openraft::raft::VoteRequest<NodeId>,
    ) -> Result<
        openraft::raft::VoteResponse<NodeId>,
        RPCError<NodeId, RaftNode, openraft::error::VoteError>,
    > {
        let address = self.get_node_address(target).await.ok_or_else(|| {
            RPCError::Network(NetworkError::new(&Unreachable::new(&format!(
                "Node {} not found in network",
                target
            ))))
        })?;

        let url = self.build_url(&address, "raft/vote");
        
        let request = VoteRequest {
            vote: rpc.vote,
            last_log_id: rpc.last_log_id,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                RPCError::Network(NetworkError::new(&Unreachable::new(&e.to_string())))
            })?;

        if !response.status().is_success() {
            return Err(RPCError::Network(NetworkError::new(&Unreachable::new(
                &format!("HTTP error: {}", response.status()),
            ))));
        }

        let vote_response: VoteResponse = response.json().await.map_err(|e| {
            RPCError::Network(NetworkError::new(&Unreachable::new(&e.to_string())))
        })?;

        Ok(openraft::raft::VoteResponse {
            vote: vote_response.vote,
            vote_granted: vote_response.vote_granted,
            last_log_id: vote_response.last_log_id,
        })
    }
}

/// Network factory for creating network instances
#[derive(Debug, Clone, Default)]
pub struct OpenRaftNetworkFactory;

#[async_trait]
impl RaftNetworkFactory<RaftRequest> for OpenRaftNetworkFactory {
    type Network = OpenRaftNetwork;

    async fn new_client(&mut self, _target: NodeId, _node: &RaftNode) -> Self::Network {
        OpenRaftNetwork::new()
    }
}

impl Default for OpenRaftNetwork {
    fn default() -> Self {
        Self::new()
    }
}
