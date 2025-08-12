use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub key: String,
    pub class_key: String,
    pub deployment_id: String,
    pub status: RuntimeStatus,
    pub health: HealthStatus,
    pub metrics: RuntimeMetrics,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub request_count: u64,
    pub error_count: u64,
    pub average_latency: f64,
}
