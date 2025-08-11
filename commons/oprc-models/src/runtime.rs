use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use validator::Validate;
use std::collections::HashMap;

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
