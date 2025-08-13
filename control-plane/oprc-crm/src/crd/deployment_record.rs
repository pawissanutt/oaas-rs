use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "oaas.io",
    version = "v1alpha1",
    kind = "DeploymentRecord",
    plural = "deploymentrecords",
    namespaced,
    status = "DeploymentRecordStatus"
)]
pub struct DeploymentRecordSpec {
    pub selected_template: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, Default)]
pub struct DeploymentRecordStatus {
    pub phase: Option<String>,
    pub message: Option<String>,
    pub observed_generation: Option<i64>,
    pub last_updated: Option<String>,
    pub conditions: Option<Vec<Condition>>, // K8s-style conditions (Progressing/Available/Degraded)
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct Condition {
    #[serde(rename = "type")]
    pub type_: ConditionType,
    pub status: ConditionStatus,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub last_transition_time: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionType {
    Available,
    Progressing,
    Degraded,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionStatus {
    True,
    False,
    Unknown,
}
