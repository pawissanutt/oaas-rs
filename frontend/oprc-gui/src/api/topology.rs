//! Topology API client (CSR)

use crate::types::{TopologyRequest, TopologySource};
use oprc_grpc::proto::topology::TopologySnapshot;

pub async fn proxy_topology(
    request: TopologyRequest,
) -> Result<TopologySnapshot, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();

    // Build URL with source query parameter
    let source = match request.source {
        TopologySource::Zenoh => "zenoh",
        TopologySource::Deployments => "deployments",
    };
    let url = format!("{}/api/v1/topology?source={}", base, source);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("API error: {}", response.status()));
    }

    let snapshot = response
        .json::<TopologySnapshot>()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    Ok(snapshot)
}
