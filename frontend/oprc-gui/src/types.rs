//! Shared type definitions for API requests and responses

use serde::{Deserialize, Serialize};

// Re-export types from oprc-grpc and oprc-models
pub use oprc_grpc::{InvocationResponse, ObjData};
pub use oprc_models::{DeploymentCondition, OClassDeployment};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OBJECT LISTING TYPES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Single object item in list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectListItem {
    pub object_id: String,
    pub version: u64,
    pub entry_count: u64,
    /// Partition ID (useful when listing across multiple partitions)
    #[serde(default)]
    pub partition_id: u32,
}

/// Response envelope for list objects API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListObjectsResponse {
    pub objects: Vec<ObjectListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// INVOCATION TYPES (client-side request wrappers)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeRequest {
    pub class_key: String,
    pub partition_id: String,
    pub function_key: String,
    pub payload: serde_json::Value,
    /// Optional: for stateful invocation on a specific object
    pub object_id: Option<String>,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OBJECT TYPES (client-side request wrappers)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectGetRequest {
    pub class_key: String,
    pub partition_id: String,
    pub object_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(not(feature = "server"), allow(dead_code))]
pub struct ObjectPutRequest {
    pub class_key: String,
    pub partition_id: String,
    pub object_id: String,
    pub data: ObjData,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// TOPOLOGY TYPES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

// Re-export topology types from oprc-grpc
pub use oprc_grpc::proto::topology::TopologySnapshot;

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum TopologySource {
    #[default]
    Deployments,
    Zenoh,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopologyRequest {
    #[serde(default)]
    pub source: TopologySource,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CLASS RUNTIME TYPES (from PM API)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Runtime class information from PM API (includes partition_count)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassRuntime {
    pub id: String,
    pub deployment_unit_id: String,
    pub package_name: String,
    pub class_key: String,
    pub target_environment: String,
    pub cluster_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Number of partitions for ODGM collections (default 1)
    #[serde(default = "default_partition_count")]
    pub partition_count: u32,
}

fn default_partition_count() -> u32 {
    1
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// PACKAGE LISTING TYPES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagesSnapshot {
    pub packages: Vec<oprc_models::OPackage>,
    pub timestamp: String,
}
