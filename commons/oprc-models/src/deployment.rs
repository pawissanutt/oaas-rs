use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use validator::Validate;
use crate::nfr::NfrRequirements;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OClassDeployment {
    #[validate(length(min = 1, message = "Deployment key cannot be empty"))]
    pub key: String,
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub package_name: String,
    #[validate(length(min = 1, message = "Class key cannot be empty"))]
    pub class_key: String,
    #[validate(length(min = 1, message = "Target environment cannot be empty"))]
    pub target_env: String,
    #[validate(length(min = 1, message = "At least one target cluster must be specified"))]
    pub target_clusters: Vec<String>,
    #[validate(nested)]
    pub nfr_requirements: NfrRequirements,
    #[validate(nested)]
    pub functions: Vec<FunctionDeploymentSpec>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct DeploymentUnit {
    #[validate(length(min = 1, message = "Deployment ID cannot be empty"))]
    pub id: String,
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub package_name: String,
    #[validate(length(min = 1, message = "Class key cannot be empty"))]
    pub class_key: String,
    #[validate(length(min = 1, message = "Target cluster cannot be empty"))]
    pub target_cluster: String,
    #[validate(nested)]
    pub functions: Vec<FunctionDeploymentSpec>,
    #[validate(length(min = 1, message = "Target environment cannot be empty"))]
    pub target_env: String,
    #[validate(nested)]
    pub nfr_requirements: NfrRequirements,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct FunctionDeploymentSpec {
    #[validate(length(min = 1, message = "Function key cannot be empty"))]
    pub function_key: String,
    #[validate(range(min = 1, message = "Replicas must be at least 1"))]
    pub replicas: u32,
    #[validate(nested)]
    pub resource_requirements: crate::ResourceRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeploymentStatus {
    Pending,
    InProgress,
    Deployed,
    Failed,
    Scaling,
    Terminating,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentFilter {
    pub package_name: Option<String>,
    pub class_key: Option<String>,
    pub target_env: Option<String>,
    pub status: Option<DeploymentStatus>,
}
