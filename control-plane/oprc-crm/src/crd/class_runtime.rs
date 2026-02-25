use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(
    CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema, Default,
)]
#[kube(
    group = "oaas.io",
    version = "v1alpha1",
    kind = "ClassRuntime",
    plural = "classruntimes",
    namespaced,
    status = "ClassRuntimeStatus"
)]
pub struct ClassRuntimeSpec {
    /// Class key for reference
    pub package_class_key: Option<String>,
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
    pub enforcement: Option<NfrEnforcementSpec>,
    /// Per-ClassRuntime telemetry/observability configuration (overrides cluster defaults)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<TelemetrySpec>,
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
    /// Per-function predicted/observed routing + readiness metadata (additive; optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions: Option<Vec<FunctionStatus>>,
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
    TelemetryConfigured,
    TelemetryDegraded,
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
    /// Optional ODGM log env filter (maps to ODGM_LOG), e.g. "info,openraft=info,zenoh=warn"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    /// Mapping of environment -> list of ODGM node ids (from PM)
    #[serde(
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub env_node_ids: BTreeMap<String, Vec<u64>>,
    /// Convenience selected node id for this runtime's environment
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<u64>,
    /// Optional explicit per-collection shard assignments provided by PM (one vec entry per partition)
    #[serde(
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub collection_assignments: BTreeMap<String, Vec<ShardAssignmentSpec>>,
}

#[derive(
    Deserialize, Serialize, Clone, Debug, JsonSchema, Default, PartialEq, Eq,
)]
pub struct ShardAssignmentSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replica: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shard_ids: Vec<u64>,
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
pub struct NfrEnforcementSpec {
    /// off | observe | enforce (default observe)
    pub mode: Option<String>,
    /// Which dimensions to enforce when mode=enforce (replicas|memory|cpu)
    pub dimensions: Option<Vec<String>>,
}

/// Per-ClassRuntime telemetry configuration for OpenTelemetry integration.
/// These settings override cluster-level defaults from CRM environment variables.
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct TelemetrySpec {
    /// Master toggle for telemetry. When false, no OTEL env vars are injected.
    /// When None, falls back to cluster-level `OPRC_CRM_OTEL_ENABLED`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Enable trace export (default true when telemetry enabled)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traces: Option<bool>,
    /// Enable metrics export (default true when telemetry enabled)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<bool>,
    /// Enable log export (default false due to high overhead)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logs: Option<bool>,
    /// Trace sampling rate (0.0–1.0). Maps to OTEL_TRACES_SAMPLER_ARG.
    /// Default 0.1 (10% sampling). Values > 0.5 may cause high overhead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling_rate: Option<f64>,
    /// Override service name for OTEL_SERVICE_NAME. If not set, derived from CR name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    /// Additional resource attributes injected as OTEL_RESOURCE_ATTRIBUTES.
    /// Format: "key1=value1,key2=value2"
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_attributes: BTreeMap<String, String>,
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
    /// For WASM functions: URL to fetch the .wasm module from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_module_url: Option<String>,
    /// Whether the function is stateless (default true)
    #[serde(default)]
    pub stateless: Option<bool>,
    /// Whether the function should be kept in standby
    #[serde(default)]
    pub standby: Option<bool>,
    /// Active group members (advanced)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_group: Vec<u64>,
    /// Optional function key reference for mapping (used to derive URLs)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_key: Option<String>,
}

// --- Per-function status (predicted URLs & readiness) ---
#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct FunctionStatus {
    /// Stable key (FunctionDeploymentSpec.function_key)
    pub function_key: String,
    /// Service name created for this function (dns1035 safe)
    pub service: String,
    /// Container port exposed (post-defaulting)
    pub port: u16,
    /// Predicted in-cluster HTTP URL (always present once populated)
    pub predicted_url: String,
    /// Observed URL (future: e.g. Knative domain) – omitted until available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_url: Option<String>,
    /// Template family that owns this function's runtime objects
    pub template: String,
    /// Readiness (None until observe phase runs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready: Option<bool>,
    /// Reason for last readiness transition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Message/details for last readiness transition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(
        rename = "lastTransitionTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_transition_time: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_spec_deserialize_all_fields() {
        let json = r#"{
            "enabled": true,
            "traces": true,
            "metrics": false,
            "logs": true,
            "sampling_rate": 0.5,
            "service_name": "my-service",
            "resource_attributes": {
                "team": "backend",
                "env": "prod"
            }
        }"#;

        let spec: TelemetrySpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.enabled, Some(true));
        assert_eq!(spec.traces, Some(true));
        assert_eq!(spec.metrics, Some(false));
        assert_eq!(spec.logs, Some(true));
        assert_eq!(spec.sampling_rate, Some(0.5));
        assert_eq!(spec.service_name, Some("my-service".to_string()));
        assert_eq!(spec.resource_attributes.len(), 2);
        assert_eq!(
            spec.resource_attributes.get("team"),
            Some(&"backend".to_string())
        );
    }

    #[test]
    fn telemetry_spec_deserialize_defaults_only() {
        let json = r#"{}"#;

        let spec: TelemetrySpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.enabled, None);
        assert_eq!(spec.traces, None);
        assert_eq!(spec.metrics, None);
        assert_eq!(spec.logs, None);
        assert_eq!(spec.sampling_rate, None);
        assert_eq!(spec.service_name, None);
        assert!(spec.resource_attributes.is_empty());
    }

    #[test]
    fn telemetry_spec_serialize_skips_none_fields() {
        let spec = TelemetrySpec {
            enabled: Some(true),
            traces: None,
            metrics: None,
            logs: None,
            sampling_rate: None,
            service_name: None,
            resource_attributes: BTreeMap::new(),
        };

        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"enabled\":true"));
        assert!(!json.contains("traces"));
        assert!(!json.contains("metrics"));
        assert!(!json.contains("logs"));
        assert!(!json.contains("sampling_rate"));
        assert!(!json.contains("service_name"));
        assert!(!json.contains("resource_attributes"));
    }

    #[test]
    fn telemetry_spec_roundtrip() {
        let mut attrs = BTreeMap::new();
        attrs.insert("key1".to_string(), "value1".to_string());

        let original = TelemetrySpec {
            enabled: Some(false),
            traces: Some(false),
            metrics: Some(true),
            logs: Some(false),
            sampling_rate: Some(0.25),
            service_name: Some("test".to_string()),
            resource_attributes: attrs,
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TelemetrySpec = serde_json::from_str(&json).unwrap();

        assert_eq!(original.enabled, deserialized.enabled);
        assert_eq!(original.traces, deserialized.traces);
        assert_eq!(original.metrics, deserialized.metrics);
        assert_eq!(original.logs, deserialized.logs);
        assert_eq!(original.sampling_rate, deserialized.sampling_rate);
        assert_eq!(original.service_name, deserialized.service_name);
        assert_eq!(
            original.resource_attributes,
            deserialized.resource_attributes
        );
    }

    #[test]
    fn function_route_wasm_module_url_roundtrip() {
        let route = FunctionRoute {
            url: "wasm://transform".to_string(),
            wasm_module_url: Some(
                "https://registry.example.com/transform.wasm".to_string(),
            ),
            stateless: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&route).unwrap();
        assert!(json.contains("wasm_module_url"));
        let back: FunctionRoute = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.wasm_module_url,
            Some("https://registry.example.com/transform.wasm".to_string())
        );
        assert_eq!(back.url, "wasm://transform");
    }

    #[test]
    fn function_route_without_wasm_url_skips_field() {
        let route = FunctionRoute {
            url: "http://svc:80".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&route).unwrap();
        assert!(!json.contains("wasm_module_url"));
    }
}
