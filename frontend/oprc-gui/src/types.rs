//! Shared type definitions for API requests and responses

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export types from oprc-grpc and oprc-models
pub use oprc_grpc::{InvocationResponse, ObjData, ObjMeta, ValData};
pub use oprc_models::{DeploymentCondition, NfrRequirements, OClassDeployment};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<(String, String)>, // (from_id, to_id)
    pub timestamp: String,
}
