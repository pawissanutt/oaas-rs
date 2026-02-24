use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub enum FunctionType {
    #[serde(rename = "BUILTIN")]
    Builtin,
    #[serde(rename = "CUSTOM")]
    Custom,
    #[serde(rename = "MACRO")]
    Macro,
    #[serde(rename = "LOGICAL")]
    Logical,
    #[serde(rename = "WASM")]
    Wasm,
}

impl Default for FunctionType {
    fn default() -> Self {
        Self::Custom
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_type_wasm_serializes_to_wasm_string() {
        let json = serde_json::to_string(&FunctionType::Wasm).unwrap();
        assert_eq!(json, r#""WASM""#);
    }

    #[test]
    fn function_type_wasm_deserializes_from_wasm_string() {
        let ft: FunctionType = serde_json::from_str(r#""WASM""#).unwrap();
        assert_eq!(ft, FunctionType::Wasm);
    }

    #[test]
    fn function_type_default_is_custom() {
        assert_eq!(FunctionType::default(), FunctionType::Custom);
    }

    #[test]
    fn function_type_roundtrip_all_variants() {
        let variants = vec![
            FunctionType::Builtin,
            FunctionType::Custom,
            FunctionType::Macro,
            FunctionType::Logical,
            FunctionType::Wasm,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let back: FunctionType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, back);
        }
    }
}
