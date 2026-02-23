use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, JsonSchema,
)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct NfrRequirements {
    #[validate(range(
        min = 1,
        message = "Min throughput must be greater than 0"
    ))]
    pub min_throughput_rps: Option<u32>,
    #[validate(range(
        min = 0.0,
        max = 1.0,
        message = "Availability must be between 0 and 1"
    ))]
    pub availability: Option<f64>,
    #[validate(range(
        min = 0.0,
        max = 1.0,
        message = "CPU utilization target must be between 0 and 1"
    ))]
    pub cpu_utilization_target: Option<f64>,
}

impl Default for NfrRequirements {
    fn default() -> Self {
        Self {
            min_throughput_rps: None,
            availability: None,
            cpu_utilization_target: None,
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, JsonSchema,
)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct QosRequirement {
    #[serde(default)]
    pub throughput: u32, // Requests per second
    #[validate(range(
        min = 0.0,
        max = 1.0,
        message = "Availability must be between 0 and 1"
    ))]
    pub availability: f64, // Availability percentage
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, JsonSchema,
)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct ProvisionConfig {
    /// Explicit container image for the function runtime (required upstream when deploying)
    /// If None, deployment controllers will reject the spec instead of applying fallbacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_image: Option<String>,
    /// URL to a compiled WASI component (.wasm). HTTP, OCI, or file:// path.
    /// Used when function_type is WASM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_module_url: Option<String>,
    pub port: Option<u16>, // Port to expose for the function
    #[serde(default)]
    pub max_concurrency: u32, // Maximum concurrent executions, 0 is not limited
    #[serde(default)]
    pub need_http2: bool, // Whether to must use HTTP/2 for the function (e.g., gRPC)
    pub cpu_request: Option<String>,
    pub memory_request: Option<String>,
    pub cpu_limit: Option<String>,
    pub memory_limit: Option<String>,
    pub min_scale: Option<u32>, // Minimum scale for autoscaling
    pub max_scale: Option<u32>, // Maximum scale for autoscaling
}

impl Default for ProvisionConfig {
    fn default() -> Self {
        Self {
            container_image: None,
            wasm_module_url: None,
            port: None,
            need_http2: false,
            max_concurrency: 0, // No limit by default
            cpu_request: None,
            memory_request: None,
            cpu_limit: None,
            memory_limit: None,
            min_scale: None,
            max_scale: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provision_config_default_has_no_wasm_url() {
        let config = ProvisionConfig::default();
        assert_eq!(config.wasm_module_url, None);
    }

    #[test]
    fn provision_config_wasm_url_roundtrip() {
        let config = ProvisionConfig {
            wasm_module_url: Some(
                "https://registry.example.com/fn.wasm".to_string(),
            ),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("wasm_module_url"));
        let back: ProvisionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.wasm_module_url,
            Some("https://registry.example.com/fn.wasm".to_string())
        );
    }

    #[test]
    fn provision_config_without_wasm_url_skips_field() {
        let config = ProvisionConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("wasm_module_url"));
    }
}
