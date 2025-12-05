use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FunctionType {
    #[serde(rename = "BUILTIN", alias = "builtin", alias = "Builtin")]
    Builtin,
    #[serde(rename = "CUSTOM", alias = "custom", alias = "Custom")]
    Custom,
    #[serde(rename = "MACRO", alias = "macro", alias = "Macro")]
    Macro,
    #[serde(rename = "LOGICAL", alias = "logical", alias = "Logical")]
    Logical,
}

impl Default for FunctionType {
    fn default() -> Self {
        Self::Custom
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeploymentCondition {
    #[serde(rename = "PENDING", alias = "pending", alias = "Pending")]
    Pending,
    #[serde(rename = "DEPLOYING", alias = "deploying", alias = "Deploying")]
    Deploying,
    #[serde(rename = "RUNNING", alias = "running", alias = "Running")]
    Running,
    #[serde(rename = "DOWN", alias = "down", alias = "Down")]
    Down,
    #[serde(rename = "DELETED", alias = "deleted", alias = "Deleted")]
    Deleted,
}

impl Default for DeploymentCondition {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FunctionAccessModifier {
    #[serde(rename = "PUBLIC", alias = "public", alias = "Public")]
    Public,
    #[serde(rename = "INTERNAL", alias = "internal", alias = "Internal")]
    Internal,
    #[serde(rename = "PRIVATE", alias = "private", alias = "Private")]
    Private,
}

impl Default for FunctionAccessModifier {
    fn default() -> Self {
        Self::Public
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConsistencyModel {
    #[serde(rename = "NONE", alias = "none", alias = "None")]
    None,
    #[serde(rename = "READ_YOUR_WRITE", alias = "read_your_write", alias = "ReadYourWrite")]
    ReadYourWrite,
    #[serde(rename = "BOUNDED_STALENESS", alias = "bounded_staleness", alias = "BoundedStaleness")]
    BoundedStaleness,
    #[serde(rename = "STRONG", alias = "strong", alias = "Strong")]
    Strong,
}

impl Default for ConsistencyModel {
    fn default() -> Self {
        Self::None
    }
}
