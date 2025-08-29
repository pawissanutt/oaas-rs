use crate::enums::DeploymentCondition;
use crate::nfr::{NfrRequirements, QosRequirement};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OClassDeployment {
    #[validate(length(min = 1, message = "Deployment key cannot be empty"))]
    pub key: String,
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub package_name: String,
    #[validate(length(min = 1, message = "Class key cannot be empty"))]
    pub class_key: String,
    #[validate(length(
        min = 1,
        message = "Target environment cannot be empty"
    ))]
    pub target_env: String,
    #[validate(length(
        min = 1,
        message = "At least one target cluster must be specified"
    ))]
    pub target_clusters: Vec<String>,
    #[validate(nested)]
    pub nfr_requirements: NfrRequirements,
    #[validate(nested)]
    pub functions: Vec<FunctionDeploymentSpec>,
    pub condition: DeploymentCondition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub odgm: Option<OdgmDataSpec>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, JsonSchema,
)]
pub struct FunctionDeploymentSpec {
    #[validate(length(min = 1, message = "Function key cannot be empty"))]
    pub function_key: String,
    /// Short human-readable description for the function
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional available location / environment name where this function may run (e.g., "edge", "cloud")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_location: Option<String>,
    /// Per-function QoS requirements (inherited from package metadata or analyzer)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qos_requirement: Option<QosRequirement>,
    /// Optional provision configuration copied from package (container image, ports, knative hints)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision_config: Option<crate::nfr::ProvisionConfig>,
    /// Arbitrary config key/value pairs from package metadata (injected as ENV to runtimes)
    #[serde(
        default,
        skip_serializing_if = "std::collections::HashMap::is_empty"
    )]
    pub config: std::collections::HashMap<String, String>,
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
            odgm: None,
            created_at: now,
            updated_at: now,
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

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, Default,
)]
pub struct OdgmDataSpec {
    /// Logical ODGM collection names to materialize. A minimal CreateCollectionRequest will
    /// be generated per name with uniform partition/replica/shard settings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<String>,
    /// Desired partition count per collection (>=1). Partitions drive parallelism and hash space.
    #[validate(range(min = 1))]
    pub partition_count: i32,
    /// Desired replica count per partition (>=1). PM selects based on availability NFRs.
    #[validate(range(min = 1))]
    pub replica_count: i32,
    /// Shard implementation / consistency strategy (e.g. "mst", "raft").
    #[validate(length(min = 1))]
    pub shard_type: String,
}
