use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FunctionType {
    #[serde(rename = "BUILTIN")]
    Builtin,
    #[serde(rename = "CUSTOM")]
    Custom,
    #[serde(rename = "MACRO")]
    Macro,
    #[serde(rename = "LOGICAL")]
    Logical,
}

impl Default for FunctionType {
    fn default() -> Self {
        Self::Custom
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeploymentCondition {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "DEPLOYING")]
    Deploying,
    #[serde(rename = "RUNNING")]
    Running,
    #[serde(rename = "DOWN")]
    Down,
    #[serde(rename = "DELETED")]
    Deleted,
}

impl Default for DeploymentCondition {
    fn default() -> Self {
        Self::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FunctionAccessModifier {
    #[serde(rename = "PUBLIC")]
    Public,
    #[serde(rename = "INTERNAL")]
    Internal,
    #[serde(rename = "PRIVATE")]
    Private,
}

impl Default for FunctionAccessModifier {
    fn default() -> Self {
        Self::Public
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConsistencyModel {
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "READ_YOUR_WRITE")]
    ReadYourWrite,
    #[serde(rename = "BOUNDED_STALENESS")]
    BoundedStaleness,
    #[serde(rename = "STRONG")]
    Strong,
}

impl Default for ConsistencyModel {
    fn default() -> Self {
        Self::None
    }
}
