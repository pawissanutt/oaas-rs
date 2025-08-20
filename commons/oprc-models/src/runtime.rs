use crate::enums::DeploymentCondition;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OClassRuntime {
    #[validate(length(min = 1, message = "Runtime key cannot be empty"))]
    pub key: String,
    #[validate(length(min = 1, message = "Class key cannot be empty"))]
    pub class_key: String,
    #[validate(length(min = 1, message = "Deployment ID cannot be empty"))]
    pub deployment_id: String,
    pub condition: DeploymentCondition,
    pub last_update: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct RuntimeState {
    #[validate(length(min = 1, message = "Instance ID cannot be empty"))]
    pub instance_id: String,
    #[validate(length(min = 1, message = "Deployment ID cannot be empty"))]
    pub deployment_id: String,
    pub status: RuntimeStatus,
    #[validate(nested)]
    pub current_resources: crate::ResourceRequirements,
    pub last_heartbeat: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RuntimeStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeFilter {
    pub deployment_id: Option<String>,
    pub status: Option<RuntimeStatus>,
    pub cluster: Option<String>,
}

impl Default for OClassRuntime {
    fn default() -> Self {
        Self {
            key: String::new(),
            class_key: String::new(),
            deployment_id: String::new(),
            condition: DeploymentCondition::Pending,
            last_update: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            instance_id: String::new(),
            deployment_id: String::new(),
            status: RuntimeStatus::Unknown,
            current_resources: crate::ResourceRequirements::default(),
            last_heartbeat: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}
