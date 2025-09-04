use schemars::JsonSchema;
use std::collections::HashMap;

use crate::deployment::OClassDeployment;
use crate::enums::*;
use crate::nfr::*;
use crate::validation::ValidationError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OPackage {
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub name: String,
    pub version: Option<String>,
    #[validate(nested)]
    pub metadata: PackageMetadata,
    #[validate(nested)]
    pub classes: Vec<OClass>,
    #[validate(nested)]
    pub functions: Vec<OFunction>,
    pub dependencies: Vec<String>,
    #[validate(nested)]
    pub deployments: Vec<OClassDeployment>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Validate, JsonSchema,
)]
pub struct ResourceRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_limit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<String>,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            cpu_request: None,
            memory_request: None,
            cpu_limit: None,
            memory_limit: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OClass {
    #[validate(length(min = 1, message = "Class key cannot be empty"))]
    pub key: String,
    pub description: Option<String>,
    #[validate(nested)]
    pub state_spec: Option<StateSpecification>,
    #[validate(nested)]
    pub function_bindings: Vec<FunctionBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OFunction {
    #[validate(length(min = 1, message = "Function key cannot be empty"))]
    pub key: String,
    #[serde(default)]
    pub function_type: FunctionType,
    pub description: Option<String>,
    #[validate(nested)]
    pub provision_config: Option<ProvisionConfig>,
    #[serde(default)]
    pub config: HashMap<String, String>, // Additional config key-value pairs (injected via ENV)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct PackageMetadata {
    pub author: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct StateSpecification {
    pub key_specs: Vec<KeySpecification>,
    pub default_provider: String,
    pub consistency_model: ConsistencyModel,
    pub persistent: bool,
    pub serialization_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct KeySpecification {
    #[validate(length(min = 1, message = "Key name cannot be empty"))]
    pub name: String,
    pub key_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct FunctionBinding {
    #[validate(length(min = 1, message = "Binding name cannot be empty"))]
    pub name: String,
    #[validate(length(min = 1, message = "Function key cannot be empty"))]
    pub function_key: String,
    pub access_modifier: FunctionAccessModifier,
    #[serde(default)]
    pub stateless: bool,
    pub parameters: Vec<String>,
}

impl Default for StateSpecification {
    fn default() -> Self {
        Self {
            key_specs: Vec::new(),
            default_provider: "memory".to_string(),
            consistency_model: ConsistencyModel::None,
            persistent: false,
            serialization_format: "json".to_string(),
        }
    }
}

impl Default for FunctionBinding {
    fn default() -> Self {
        Self {
            name: String::new(),
            function_key: String::new(),
            access_modifier: FunctionAccessModifier::Public,
            stateless: false,
            parameters: Vec::new(),
        }
    }
}

impl OPackage {
    /// Additional validation beyond what the validator provides
    pub fn validate_business_rules(&self) -> Result<(), ValidationError> {
        // Validate class keys are unique
        let mut class_keys = std::collections::HashSet::new();
        for class in &self.classes {
            if !class_keys.insert(&class.key) {
                return Err(ValidationError::DuplicateClassKey(
                    class.key.clone(),
                ));
            }
        }

        // Validate function keys are unique
        let mut function_keys = std::collections::HashSet::new();
        for function in &self.functions {
            if !function_keys.insert(&function.key) {
                return Err(ValidationError::DuplicateFunctionKey(
                    function.key.clone(),
                ));
            }
        }

        // Validate dependencies exist and are not empty
        for dependency in &self.dependencies {
            if dependency.is_empty() {
                return Err(ValidationError::EmptyDependencyName);
            }
        }

        Ok(())
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
