//! Environments API proxy

use oprc_models::{ClusterHealth, ClusterInfo};

/// Fetch all environments (clusters) with their health status
pub async fn proxy_environments() -> Result<Vec<ClusterInfo>, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/envs", base);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    resp.json().await.map_err(|e| anyhow::anyhow!(e))
}

/// Fetch health for all clusters (more detailed)
#[allow(dead_code)]
pub async fn proxy_environments_health() -> Result<Vec<ClusterHealth>, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/envs/health", base);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    resp.json().await.map_err(|e| anyhow::anyhow!(e))
}

/// Fetch health for a specific cluster
#[allow(dead_code)]
pub async fn proxy_cluster_health(cluster_name: &str) -> Result<ClusterHealth, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/envs/{}/health", base, cluster_name);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }

    resp.json().await.map_err(|e| anyhow::anyhow!(e))
}
