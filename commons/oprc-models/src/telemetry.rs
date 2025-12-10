//! Telemetry configuration models for deployments.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-deployment telemetry configuration for OpenTelemetry integration.
/// Maps to ClassRuntime TelemetrySpec in the CRM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, JsonSchema)]
pub struct TelemetryConfig {
    /// Master toggle for telemetry. When false, no OTEL env vars are injected.
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
    /// Override service name for OTEL_SERVICE_NAME. If not set, derived from deployment key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    /// Additional resource attributes injected as OTEL_RESOURCE_ATTRIBUTES.
    /// Format: "key1=value1,key2=value2"
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub resource_attributes: HashMap<String, String>,
}
