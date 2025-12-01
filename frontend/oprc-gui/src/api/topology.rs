//! Topology API client (CSR)

use crate::types::{
    TopologyEdge, TopologyNode, TopologyRequest, TopologySnapshot,
};
use dioxus::prelude::*;
use oprc_grpc::proto::topology::TopologySnapshot as GrpcTopologySnapshot;

pub async fn proxy_topology(
    _request: TopologyRequest,
) -> Result<TopologySnapshot, anyhow::Error> {
    let client = reqwest::Client::new();
    // In CSR mode, we assume the UI is served from the same origin as the API (via PM)
    // or proxied correctly.
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/topology", base);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("API error: {}", response.status()));
    }

    let grpc_snapshot = response
        .json::<GrpcTopologySnapshot>()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let nodes = grpc_snapshot
        .nodes
        .into_iter()
        .map(|n| TopologyNode {
            id: n.id,
            node_type: n.node_type,
            status: n.status,
            metadata: n.metadata,
            deployed_classes: n.deployed_classes,
        })
        .collect();

    let edges = grpc_snapshot
        .edges
        .into_iter()
        .map(|e| TopologyEdge {
            from_id: e.from_id,
            to_id: e.to_id,
            metadata: e.metadata,
        })
        .collect();

    // Simple timestamp conversion
    let timestamp = grpc_snapshot.timestamp;

    Ok(TopologySnapshot {
        nodes,
        edges,
        timestamp,
    })
}
