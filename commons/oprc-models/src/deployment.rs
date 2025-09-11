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
    /// Explicit target environments to deploy to. If empty, the system will
    /// select environments automatically based on availability and NFRs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_envs: Vec<String>,
    /// Optional allow-list of environments that are eligible for automatic
    /// selection. If empty, all known environments are considered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_envs: Vec<String>,
    #[validate(nested)]
    #[serde(default)]
    pub nfr_requirements: NfrRequirements,
    #[validate(nested)]
    #[serde(default)]
    pub functions: Vec<FunctionDeploymentSpec>,
    #[serde(default)]
    pub condition: DeploymentCondition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub odgm: Option<OdgmDataSpec>,
    /// Optional runtime status summary populated by the Package Manager.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<DeploymentStatusSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
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
            target_envs: Vec::new(),
            available_envs: Vec::new(),
            nfr_requirements: NfrRequirements::default(),
            functions: Vec::new(),
            condition: DeploymentCondition::Pending,
            odgm: None,
            status: None,
            created_at: Some(now),
            updated_at: Some(now),
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

impl Default for DeploymentFilter {
    fn default() -> Self {
        Self {
            package_name: None,
            class_key: None,
            target_env: None,
            condition: None,
        }
    }
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
    /// Optional ODGM log env filter (maps to ODGM_LOG), e.g. "info,openraft=info,zenoh=warn"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Default, JsonSchema,
)]
pub struct DeploymentStatusSummary {
    /// Chosen replication factor (number of environments) for this deployment.
    pub replication_factor: u32,
    /// The concrete environments where the deployment is (or will be) placed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_envs: Vec<String>,
    /// Best-effort achieved quorum availability for the selected environments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub achieved_quorum_availability: Option<f64>,
    /// Optional last error observed during scheduling or rollout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}
