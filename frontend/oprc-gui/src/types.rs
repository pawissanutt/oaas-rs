//! Shared type definitions for API requests and responses

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export types from oprc-grpc and oprc-models
pub use oprc_grpc::{InvocationResponse, ObjData};
pub use oprc_models::{DeploymentCondition, OClassDeployment};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    pub id: String,
    pub node_type: String, // "gateway", "router", "odgm", "function"
    pub status: String,    // "healthy", "degraded", "down"
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub deployed_classes: Vec<String>, // Classes deployed on this node (for ODGM/function nodes)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<(String, String)>, // (from_id, to_id)
    pub timestamp: String,
}

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
// PACKAGE LISTING TYPES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageFunctionInfo {
    pub key: String,
    pub function_type: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageClassInfo {
    pub key: String,
    pub description: Option<String>,
    pub stateless_functions: Vec<String>,
    pub stateful_functions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub version: Option<String>,
    pub classes: Vec<PackageClassInfo>,
    pub functions: Vec<PackageFunctionInfo>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagesSnapshot {
    pub packages: Vec<PackageInfo>,
    pub timestamp: String,
}
