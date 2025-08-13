use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(
    group = "oaas.io",
    version = "v1alpha1",
    kind = "DeploymentTemplate",
    plural = "deploymenttemplates",
    namespaced
)]
pub struct DeploymentTemplateSpec {
    pub name: String,
    pub description: Option<String>,
    pub template_type: String,
}
