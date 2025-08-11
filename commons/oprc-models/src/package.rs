use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use validator::Validate;
use crate::validation::ValidationError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OPackage {
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub name: String,
    pub version: Option<String>,
    #[validate(nested)]
    pub classes: Vec<OClass>,
    #[validate(nested)]
    pub functions: Vec<OFunction>,
    pub dependencies: Vec<String>,
    #[validate(nested)]
    pub metadata: PackageMetadata,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OClass {
    #[validate(length(min = 1, message = "Class key cannot be empty"))]
    pub key: String,
    #[validate(length(min = 1, message = "Class name cannot be empty"))]
    pub name: String,
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub package: String,
    #[validate(length(min = 1, message = "Function bindings cannot be empty"))]
    pub functions: Vec<FunctionBinding>,
    pub state_spec: Option<StateSpecification>,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct OFunction {
    #[validate(length(min = 1, message = "Function key cannot be empty"))]
    pub key: String,
    #[validate(length(min = 1, message = "Function name cannot be empty"))]
    pub name: String,
    #[validate(length(min = 1, message = "Package name cannot be empty"))]
    pub package: String,
    #[validate(length(min = 1, message = "Runtime cannot be empty"))]
    pub runtime: String,
    #[validate(length(min = 1, message = "Handler cannot be empty"))]
    pub handler: String,
    #[validate(nested)]
    pub metadata: FunctionMetadata,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct PackageMetadata {
    pub author: String,
    pub description: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct FunctionMetadata {
    pub description: String,
    pub parameters: Vec<String>,
    pub return_type: String,
    pub resource_requirements: ResourceRequirements,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct StateSpecification {
    pub state_type: String,
    pub persistent: bool,
    pub serialization_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct FunctionBinding {
    pub function_key: String,
    pub access_modifier: String,
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Validate)]
pub struct ResourceRequirements {
    pub cpu_request: String,
    pub memory_request: String,
    pub cpu_limit: Option<String>,
    pub memory_limit: Option<String>,
}

impl OPackage {
    /// Additional validation beyond what the validator provides
    pub fn validate_business_rules(&self) -> Result<(), ValidationError> {
        // Validate class keys are unique
        let mut class_keys = std::collections::HashSet::new();
        for class in &self.classes {
            if !class_keys.insert(&class.key) {
                return Err(ValidationError::DuplicateClassKey(class.key.clone()));
            }
        }
        
        // Validate function keys are unique
        let mut function_keys = std::collections::HashSet::new();
        for function in &self.functions {
            if !function_keys.insert(&function.key) {
                return Err(ValidationError::DuplicateFunctionKey(function.key.clone()));
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
