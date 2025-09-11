use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "oaas.io",
    version = "v1alpha1",
    kind = "ClassRuntime",
    plural = "classruntimes",
    namespaced,
    status = "ClassRuntimeStatus"
)]
pub struct ClassRuntimeSpec {
    /// Optional explicit template selection (e.g., "dev", "edge", "cloud")
    pub selected_template: Option<String>,
    /// Simple addon list; defaults to ["odgm"] when omitted to enable core data services.
    #[serde(default = "default_addons")]
    pub addons: Option<Vec<String>>,
    /// ODGM configuration, currently collection-focused
    pub odgm_config: Option<OdgmConfigSpec>,
    /// Function runtime container hints - allow multiple functions per record
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<FunctionSpec>,
    /// Non-functional requirements to guide template selection (heuristic)
    pub nfr_requirements: Option<NfrRequirementsSpec>,
    /// NFR configuration (enforcement toggles/mode); observe-only by default
    pub nfr: Option<NfrSpec>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct ClassRuntimeStatus {
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
    /// Observe-only recommendations produced by the analyzer (M4).
    /// The Helm chart CRD used in tests expects this to be an object mapping
    /// (e.g., { "replicas": 3 }). Use a typed map so the CRD schema emits an
    /// object with string keys and arbitrary JSON values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nfr_recommendations: Option<BTreeMap<String, Value>>,
    /// Audit trail of the latest applied set of recommendations (M5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_recommendations: Option<Vec<NfrRecommendation>>,
    /// Timestamp when the latest recommendations were applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_applied_at: Option<String>,
    /// Discovered Zenoh router services in the namespace (observed state).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routers: Option<Vec<RouterEndpoint>>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct RouterEndpoint {
    /// Kubernetes Service name (in the same namespace)
    pub service: String,
    /// Service port used for zenoh (typically 17447)
    pub port: i32,
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
    #[serde(
        rename = "lastTransitionTime",
        skip_serializing_if = "Option::is_none"
    )]
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
    /// Desired partition count for each collection (default 1)
    pub partition_count: Option<i32>,
    /// Desired replica count per partition (provided by PM; default 1)
    pub replica_count: Option<i32>,
    /// Shard type (e.g., "mst", "raft", etc.) default "mst"
    pub shard_type: Option<String>,
    /// Optional function invocation routing configuration per collection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocations: Option<InvocationsSpec>,
    /// Additional options to pass to ODGM collection creation (maps to CreateCollectionRequest.options)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<BTreeMap<String, String>>,
}

// Reuse the shared `FunctionDeploymentSpec` from `oprc-models` so the CRM CRD
// exposes the same function metadata shape as packages/deployment units.
pub type FunctionSpec = oprc_models::FunctionDeploymentSpec;

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

// --- Defaults helpers ---
fn default_addons() -> Option<Vec<String>> {
    Some(vec!["odgm".into()])
}

// --- ODGM invocation routing config (optional) ---

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct InvocationsSpec {
    /// Map of function route IDs to their routing configuration
    #[serde(
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub fn_routes: BTreeMap<String, FunctionRoute>,
    /// List of disabled function IDs for this collection
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_fn: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct FunctionRoute {
    /// URL endpoint for the function
    pub url: String,
    /// Whether the function is stateless (default true)
    #[serde(default)]
    pub stateless: Option<bool>,
    /// Whether the function should be kept in standby
    #[serde(default)]
    pub standby: Option<bool>,
    /// Active group members (advanced)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_group: Vec<u64>,
}
