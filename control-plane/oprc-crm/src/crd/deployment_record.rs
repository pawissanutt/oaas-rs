use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "oaas.io",
    version = "v1alpha1",
    kind = "DeploymentRecord",
    plural = "deploymentrecords",
    namespaced,
    status = "DeploymentRecordStatus"
)]
pub struct DeploymentRecordSpec {
    /// Optional explicit template selection (e.g., "dev", "edge", "cloud")
    pub selected_template: Option<String>,
    /// Simple addon list; e.g., ["odgm"]
    pub addons: Option<Vec<String>>,
    /// ODGM configuration, currently collection-focused
    pub odgm_config: Option<OdgmConfigSpec>,
    /// Function runtime container hints
    pub function: Option<FunctionSpec>,
    /// Non-functional requirements to guide template selection (heuristic)
    pub nfr_requirements: Option<NfrRequirementsSpec>,
    /// NFR configuration (enforcement toggles/mode); observe-only by default
    pub nfr: Option<NfrSpec>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct DeploymentRecordStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>, // K8s-style conditions (Progressing/Available/Degraded)
    /// Provider-visible child resources for traceability (e.g., ServiceMonitor/PodMonitor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_refs: Option<Vec<ResourceRef>>,
    /// Observe-only recommendations produced by the analyzer (M4)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfr_recommendations: Option<Vec<NfrRecommendation>>,
    /// Audit trail of the latest applied set of recommendations (M5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_recommendations: Option<Vec<NfrRecommendation>>,
    /// Timestamp when the latest recommendations were applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_at: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct Condition {
    #[serde(rename = "type")]
    pub type_: ConditionType,
    pub status: ConditionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_transition_time: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionType {
    Available,
    Progressing,
    Degraded,
    NfrObserved,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionStatus {
    True,
    False,
    Unknown,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct OdgmConfigSpec {
    /// Logical collection names this Class expects
    pub collections: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct FunctionSpec {
    /// OCI image for function runtime
    pub image: Option<String>,
    /// Container port exposed by the function runtime
    pub port: Option<i32>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct NfrRequirementsSpec {
    /// Minimum target throughput in requests per second
    pub min_throughput_rps: Option<u32>,
    /// Maximum acceptable 95th percentile latency in milliseconds
    pub max_latency_ms: Option<u32>,
    /// Target availability percentage (e.g., 99.9)
    pub availability_pct: Option<f32>,
    /// Consistency preference (e.g., "eventual" or "strong")
    pub consistency: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct NfrSpec {
    pub enforcement: Option<NfrEnforcementSpec>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct NfrEnforcementSpec {
    /// off | observe | enforce (default observe)
    pub mode: Option<String>,
    /// Which dimensions to enforce when mode=enforce (replicas|memory|cpu)
    pub dimensions: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct ResourceRef {
    pub kind: String,
    pub name: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct NfrRecommendation {
    /// Component name (e.g., "function" or "odgm")
    pub component: String,
    /// Dimension: replicas|memory|cpu
    pub dimension: String,
    /// Target value (unit depends on dimension; e.g., replicas count, bytes)
    pub target: f64,
    /// Basis (e.g., "p99_latency over 10m"), optional
    pub basis: Option<String>,
    /// Confidence [0..1], optional
    pub confidence: Option<f32>,
}
