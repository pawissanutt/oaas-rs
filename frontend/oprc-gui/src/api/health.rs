//! Health proxy: fetch cluster env health from Package Manager and summarize

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "server")]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClusterHealthInfo {
    pub cluster_name: String,
    pub status: String,
    pub crm_version: Option<String>,
    pub last_seen: String,
    pub node_count: Option<u32>,
    pub ready_nodes: Option<u32>,
    pub availability: Option<f64>,
}

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

#[post("/api/proxy/system_health")]
pub async fn proxy_system_health() -> Result<SystemHealthSnapshot, ServerFnError>
{
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }
    #[cfg(feature = "server")]
    {
        use chrono::Utc;
        use reqwest::Client;

        let base = crate::config::pm_base_url();
        // Use /api/v1/envs/health returning Vec<ClusterHealth>
        let url = format!("{}/api/v1/envs/health", base);
        let client = Client::new();
        let resp = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(ServerFnError::new(format!(
                "PM envs health failed: {}",
                status
            )));
        }

        // Expect Vec<ClusterHealthInfo>
        let items: Vec<ClusterHealthInfo> = resp
            .json()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        let mut envs: Vec<EnvironmentStatus> = Vec::new();
        let mut worst_rank = 0u8; // 0 healthy, 1 degraded, 2 down

        for item in items.into_iter() {
            let raw = item.status.to_lowercase();
            let normalized = match raw.as_str() {
                "healthy" => "healthy",
                "degraded" => "degraded",
                "unhealthy" | "down" => "down",
                other => {
                    if other.contains("healthy") {
                        "healthy"
                    } else {
                        "degraded"
                    }
                }
            };
            let rank = match normalized {
                "healthy" => 0,
                "degraded" => 1,
                _ => 2,
            };
            worst_rank = worst_rank.max(rank);
            envs.push(EnvironmentStatus {
                name: item.cluster_name,
                status: normalized.to_string(),
                availability: item.availability,
                node_count: item.node_count,
                ready_nodes: item.ready_nodes,
            });
        }

        let overall = match worst_rank {
            0 => "healthy",
            1 => "degraded",
            _ => "down",
        };

        Ok(SystemHealthSnapshot {
            overall_status: overall.to_string(),
            environments: envs,
            timestamp: Utc::now().to_rfc3339(),
        })
    }
}
