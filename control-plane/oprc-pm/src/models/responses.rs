use chrono::{DateTime, Utc};
use oprc_models::DeploymentCondition;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageResponse {
    pub id: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentResponse {
    pub id: String,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub id: String,
    pub deployment_unit_id: String,
    pub package_name: String,
    pub class_key: String,
    pub target_environment: String,
    pub cluster_name: Option<String>, // Which cluster this record is from
    pub status: DeploymentRecordStatus,
    pub nfr_compliance: Option<NfrCompliance>,
    pub resource_refs: Vec<ResourceReference>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecordStatus {
    pub condition: DeploymentCondition, // Pending, Deploying, Running, Down, Deleted
    pub phase: DeploymentPhase, // TemplateSelection, ResourceProvisioning, etc.
    pub message: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeploymentPhase {
    Unknown,
    TemplateSelection,
    ResourceProvisioning,
    Enforcement,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfrCompliance {
    pub overall_status: String, // Compliant, Violation, Unknown
    pub throughput_status: Option<String>,
    pub latency_status: Option<String>,
    pub availability_status: Option<String>,
    pub last_checked: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReference {
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentStatus {
    pub id: String,
    pub status: String,
    pub phase: String,
    pub message: Option<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub name: String,
    pub health: ClusterHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    pub cluster_name: String,
    pub status: String, // Healthy, Degraded, Unhealthy
    pub crm_version: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub node_count: Option<u32>,
    pub ready_nodes: Option<u32>,
}
