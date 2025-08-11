use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct NfrRequirements {
    #[validate(range(min = 1, message = "Max latency must be greater than 0"))]
    pub max_latency_ms: Option<u32>,
    #[validate(range(min = 1, message = "Min throughput must be greater than 0"))]
    pub min_throughput_rps: Option<u32>,
    #[validate(range(min = 0.0, max = 1.0, message = "Availability must be between 0 and 1"))]
    pub availability: Option<f64>,
    #[validate(range(min = 0.0, max = 1.0, message = "CPU utilization target must be between 0 and 1"))]
    pub cpu_utilization_target: Option<f64>,
}

impl Default for NfrRequirements {
    fn default() -> Self {
        Self {
            max_latency_ms: None,
            min_throughput_rps: None,
            availability: Some(0.99), // 99% availability by default
            cpu_utilization_target: Some(0.7), // 70% CPU target by default
        }
    }
}
