use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use validator::Validate;
use crate::nfr::NfrRequirements;
use crate::enums::DeploymentCondition;

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
    pub condition: DeploymentCondition,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub condition: DeploymentCondition,
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

impl Default for OClassDeployment {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            key: String::new(),
            package_name: String::new(),
            class_key: String::new(),
            target_env: "development".to_string(),
            target_clusters: Vec::new(),
            nfr_requirements: NfrRequirements::default(),
            functions: Vec::new(),
            condition: DeploymentCondition::Pending,
            created_at: now,
            updated_at: now,
        }
    }
}

impl Default for DeploymentUnit {
    fn default() -> Self {
        Self {
            id: String::new(),
            package_name: String::new(),
            class_key: String::new(),
            target_cluster: String::new(),
            functions: Vec::new(),
            target_env: "development".to_string(),
            nfr_requirements: NfrRequirements::default(),
            condition: DeploymentCondition::Pending,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentFilter {
    pub package_name: Option<String>,
    pub class_key: Option<String>,
    pub target_env: Option<String>,
    pub condition: Option<DeploymentCondition>,
}
