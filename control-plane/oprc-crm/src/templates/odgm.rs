use k8s_openapi::api::core::v1::EnvVar;

use crate::crd::class_runtime::ClassRuntimeSpec as DeploymentRecordSpec;
use oprc_pb::CreateCollectionRequest;

use crate::collections::build_collection_request;

/// Returns non-empty ODGM collection names from the spec, if any.
pub fn collection_names(spec: &DeploymentRecordSpec) -> Option<&Vec<String>> {
    spec.odgm_config
        .as_ref()
        .and_then(|o| o.collections.as_ref())
        .filter(|v| !v.is_empty())
}

fn build_requests(
    spec: &DeploymentRecordSpec,
    names: &Vec<String>,
) -> Vec<CreateCollectionRequest> {
    let partition_count = spec
        .odgm_config
        .as_ref()
        .and_then(|c| c.partition_count)
        .unwrap_or(1);
    let replica_count = spec
        .odgm_config
        .as_ref()
        .and_then(|c| c.replica_count)
        .unwrap_or(1);
    let shard_type = spec
        .odgm_config
        .as_ref()
        .and_then(|c| c.shard_type.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("mst");
    // Capture references to invocations and options if present
    let invocations = spec
        .odgm_config
        .as_ref()
        .and_then(|c| c.invocations.as_ref());
    let options = spec.odgm_config.as_ref().and_then(|c| c.options.as_ref());

    names
        .iter()
        .map(|n| {
            build_collection_request(
                n,
                partition_count,
                replica_count,
                shard_type,
                invocations,
                options,
            )
        })
        .collect()
}

/// Build a k8s EnvVar for ODGM_COLLECTION containing JSON array of CreateCollectionRequest.
pub fn collections_env_var(
    names: &Vec<String>,
    spec: &DeploymentRecordSpec,
) -> Result<EnvVar, crate::templates::manager::TemplateError> {
    let reqs = build_requests(spec, names);
    let value = serde_json::to_string(&reqs)?;
    Ok(EnvVar {
        name: "ODGM_COLLECTION".to_string(),
        value: Some(value),
        ..Default::default()
    })
}

/// Build a serde_json env object for Knative style manifests with JSON array value.
pub fn collections_env_json(
    names: &Vec<String>,
    spec: &DeploymentRecordSpec,
) -> serde_json::Value {
    let reqs = build_requests(spec, names);
    serde_json::json!({
        "name": "ODGM_COLLECTION",
        "value": serde_json::to_string(&reqs).unwrap(),
    })
}
