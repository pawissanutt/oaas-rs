use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub details: HashMap<String, String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl HealthCheck {
    pub fn healthy() -> Self {
        Self {
            status: HealthStatus::Healthy,
            message: None,
            details: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn unhealthy(message: String) -> Self {
        Self {
            status: HealthStatus::Unhealthy,
            message: Some(message),
            details: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn with_details(mut self, details: HashMap<String, String>) -> Self {
        self.details = details;
        self
    }
}

pub trait HealthChecker: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self) -> Result<HealthCheck, Box<dyn std::error::Error>>;
}

pub struct ServiceHealthManager {
    checkers: Vec<Box<dyn HealthChecker>>,
}

impl ServiceHealthManager {
    pub fn new() -> Self {
        Self {
            checkers: Vec::new(),
        }
    }

    pub fn add_checker(mut self, checker: Box<dyn HealthChecker>) -> Self {
        self.checkers.push(checker);
        self
    }

    pub fn check_all(&self) -> HashMap<String, HealthCheck> {
        let mut results = HashMap::new();

        for checker in &self.checkers {
            let result = match checker.check() {
                Ok(health) => health,
                Err(e) => HealthCheck::unhealthy(e.to_string()),
            };
            results.insert(checker.name().to_string(), result);
        }

        results
    }

    pub fn overall_status(&self) -> HealthStatus {
        let checks = self.check_all();

        if checks.is_empty() {
            return HealthStatus::Unknown;
        }

        for (_, check) in checks {
            if check.status == HealthStatus::Unhealthy {
                return HealthStatus::Unhealthy;
            }
        }

        HealthStatus::Healthy
    }
}

impl Default for ServiceHealthManager {
    fn default() -> Self {
        Self::new()
    }
}
