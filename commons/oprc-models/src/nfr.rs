use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, JsonSchema,
)]
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
pub struct ProvisionConfig {
    /// Explicit container image for the function runtime (required upstream when deploying)
    /// If None, deployment controllers will reject the spec instead of applying fallbacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_image: Option<String>,
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
