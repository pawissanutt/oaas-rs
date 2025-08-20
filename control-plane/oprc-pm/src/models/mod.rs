pub mod filters;
pub mod responses;
pub mod runtime;

pub use filters::*;
pub use responses::*;
pub use runtime::*;

// Re-export commonly used types from oprc_models
pub use oprc_models::{
    ConsistencyModel, DeploymentCondition, DeploymentUnit,
    FunctionAccessModifier, FunctionBinding, FunctionType, KeySpecification,
    KnativeConfig, NfrRequirements, OClass, OClassDeployment, OClassRuntime,
    OFunction, OPackage, PackageMetadata, ProvisionConfig, QosRequirement,
    StateSpecification, TrafficSplit,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PackageId(pub String);

impl PackageId {
    pub fn new(name: String) -> Self {
        Self(name)
    }

    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for PackageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DeploymentId(pub String);

impl DeploymentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(id: String) -> Self {
        Self(id)
    }
}

impl Default for DeploymentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DeploymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentReplicas {
    pub min: u32,
    pub max: u32,
    pub target: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub partition_key: String,
    pub partition_count: u32,
    pub replication_factor: u32,
}
