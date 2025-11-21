use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClusterInfo {
    pub name: String,
    pub health: ClusterHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClusterHealth {
    pub cluster_name: String,
    pub status: String, // Healthy, Degraded, Unhealthy
    pub crm_version: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub node_count: Option<u32>,
    pub ready_nodes: Option<u32>,
    pub availability: Option<f64>,
}
