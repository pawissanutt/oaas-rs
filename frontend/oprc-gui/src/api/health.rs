//! Health proxy: fetch cluster env health from Package Manager and summarize

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthSnapshot {
    pub overall_status: String,
    pub environments: Vec<EnvironmentStatus>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentStatus {
    pub name: String,
    pub status: String,
    pub availability: Option<f64>,
    pub node_count: Option<u32>,
    pub ready_nodes: Option<u32>,
}

pub async fn proxy_system_health() -> Result<SystemHealthSnapshot, anyhow::Error>
{
    use chrono::Utc;
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/envs/health", base);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("API error: {}", resp.status()));
    }

    // The PM returns a list of ClusterHealth
    use oprc_models::ClusterHealth;

    let health_list: Vec<ClusterHealth> =
        resp.json().await.map_err(|e| anyhow::anyhow!(e))?;
    let mut environments = Vec::new();
    let mut overall_healthy = true;

    for h in health_list {
        let status = h.status.to_lowercase();
        if status != "healthy" {
            overall_healthy = false;
        }
        environments.push(EnvironmentStatus {
            name: h.cluster_name,
            status: h.status,
            availability: h.availability,
            node_count: h.node_count,
            ready_nodes: h.ready_nodes,
        });
    }

    Ok(SystemHealthSnapshot {
        overall_status: if overall_healthy {
            "Healthy".into()
        } else {
            "Degraded".into()
        },
        environments,
        timestamp: Utc::now().to_rfc3339(),
    })
}
