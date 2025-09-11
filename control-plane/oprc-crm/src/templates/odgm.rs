use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::EnvVar;
use k8s_openapi::api::core::v1::{
    Container, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    LabelSelector, ObjectMeta, OwnerReference,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

use crate::crd::class_runtime::ClassRuntimeSpec as DeploymentRecordSpec;
use crate::templates::manager::TemplateError;
use oprc_pb::CreateCollectionRequest;

use crate::collections::build_collection_request;

/// Returns non-empty ODGM collection names from the spec, if any.
#[inline]
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
) -> Result<EnvVar, TemplateError> {
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

/// Build a k8s EnvVar for ODGM_LOG when configured in the spec.
pub fn log_env_var(spec: &DeploymentRecordSpec) -> Option<EnvVar> {
    let value = spec
        .odgm_config
        .as_ref()
        .and_then(|c| c.log.as_ref())
        .cloned();
    value.map(|v| EnvVar {
        name: "ODGM_LOG".to_string(),
        value: Some(v),
        ..Default::default()
    })
}

/// Build a serde_json env object for ODGM_LOG (Knative style) when configured.
pub fn log_env_json(spec: &DeploymentRecordSpec) -> Option<serde_json::Value> {
    spec.odgm_config
        .as_ref()
        .and_then(|c| c.log.as_ref())
        .map(|v| {
            serde_json::json!({
                "name": "ODGM_LOG",
                "value": v,
            })
        })
}

/// Build ODGM-related EnvVars for a function container (k8s style). Returns an
/// empty Vec when the sidecar/ODGM feature is disabled.
pub fn build_function_odgm_env_k8s(
    ctx: &crate::templates::manager::RenderContext<'_>,
) -> Result<Vec<EnvVar>, TemplateError> {
    if !ctx.enable_odgm_sidecar {
        return Ok(Vec::new());
    }
    let odgm_name =
        format!("{}-odgm", crate::templates::manager::dns1035_safe(ctx.name));
    // Fixed ODGM HTTP port across all templates (previously overridable / 8081)
    let odgm_port = 8080;
    let odgm_service = format!("{}-svc:{}", odgm_name, odgm_port);
    let mut env: Vec<EnvVar> = vec![
        EnvVar {
            name: "ODGM_ENABLED".into(),
            value: Some("true".into()),
            ..Default::default()
        },
        EnvVar {
            name: "ODGM_SERVICE".into(),
            value: Some(odgm_service),
            ..Default::default()
        },
    ];
    if let Some(cols) = collection_names(ctx.spec) {
        env.push(collections_env_var(cols, ctx.spec)?);
    }
    if let Some(e) = log_env_var(ctx.spec) {
        env.push(e);
    }
    if let Some(router_name) = ctx.router_service_name.as_ref() {
        let router_port = ctx.router_service_port.unwrap_or(17447);
        let router_zenoh = format!("tcp/{}:{}", router_name, router_port);
        let odgm_zenoh = format!("tcp/{}-svc:17447", odgm_name);
        env.push(EnvVar {
            name: "OPRC_ZENOH_MODE".into(),
            value: Some("client".into()),
            ..Default::default()
        });
        env.push(EnvVar {
            name: "OPRC_ZENOH_PEERS".into(),
            value: Some(format!("{},{}", router_zenoh, odgm_zenoh)),
            ..Default::default()
        });
    }
    Ok(env)
}

/// Build ODGM-related env (JSON objects) for Knative style manifests.
pub fn build_function_odgm_env_json(
    ctx: &crate::templates::manager::RenderContext<'_>,
) -> Result<Vec<serde_json::Value>, TemplateError> {
    // Delegate to k8s-style env builder for single-source-of-truth, just
    // remapping to JSON objects and forcing the Knative port override (8080).
    let k8s_env = build_function_odgm_env_k8s(ctx)?;
    let json_env = k8s_env
        .into_iter()
        .map(|v| {
            serde_json::json!({
                "name": v.name,
                // All current vars set value=Some(..); fall back to empty string if ever None.
                "value": v.value.unwrap_or_default()
            })
        })
        .collect();
    Ok(json_env)
}

fn owner_ref(
    uid: Option<&str>,
    name: &str,
    api_version: &str,
    kind: &str,
) -> Option<Vec<OwnerReference>> {
    uid.map(|u| {
        vec![OwnerReference {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            uid: u.to_string(),
            controller: Some(true),
            block_owner_deletion: None,
        }]
    })
}

/// Build ODGM Deployment + Service pair.
pub fn build_odgm_resources(
    ctx: &crate::templates::manager::RenderContext<'_>,
    replicas: i32,
    image_override: Option<&str>,
    include_owner_refs: bool,
) -> Result<(Deployment, Service), TemplateError> {
    let odgm_name =
        format!("{}-odgm", crate::templates::manager::dns1035_safe(ctx.name));
    let mut odgm_lbls = std::collections::BTreeMap::new();
    odgm_lbls.insert("app".to_string(), odgm_name.clone());
    odgm_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
    let odgm_labels = Some(odgm_lbls.clone());
    let odgm_selector = LabelSelector {
        match_labels: odgm_labels.clone(),
        ..Default::default()
    };
    let odgm_img =
        image_override.unwrap_or("ghcr.io/pawissanutt/oaas/odgm:latest");
    let odgm_port = 8080; // fixed ODGM HTTP port
    let mut odgm_container = Container {
        name: "odgm".into(),
        image: Some(odgm_img.to_string()),
        ports: Some(vec![
            k8s_openapi::api::core::v1::ContainerPort {
                container_port: odgm_port,
                ..Default::default()
            },
            k8s_openapi::api::core::v1::ContainerPort {
                container_port: 17447,
                name: Some("zenoh".into()),
                ..Default::default()
            },
        ]),
        env: Some(vec![EnvVar {
            name: "ODGM_CLUSTER_ID".into(),
            value: Some(ctx.name.to_string()),
            ..Default::default()
        }]),
        ..Default::default()
    };
    if let Some(cols) = collection_names(ctx.spec) {
        let mut env = odgm_container.env.take().unwrap_or_default();
        env.push(collections_env_var(cols, ctx.spec)?);
        if let Some(e) = log_env_var(ctx.spec) {
            env.push(e);
        }
        if let Some(router_name) = ctx.router_service_name.as_ref() {
            let router_port = ctx.router_service_port.unwrap_or(17447);
            env.push(EnvVar {
                name: "OPRC_ZENOH_MODE".into(),
                value: Some("peer".into()),
                ..Default::default()
            });
            env.push(EnvVar {
                name: "OPRC_ZENOH_PORT".into(),
                value: Some("17447".into()),
                ..Default::default()
            });
            env.push(EnvVar {
                name: "OPRC_ZENOH_PEERS".into(),
                value: Some(format!("tcp/{}:{}", router_name, router_port)),
                ..Default::default()
            });
        }
        odgm_container.env = Some(env);
    }
    let owner_refs = if include_owner_refs {
        owner_ref(
            ctx.owner_uid,
            ctx.name,
            ctx.owner_api_version,
            ctx.owner_kind,
        )
    } else {
        None
    };
    let odgm_deployment = Deployment {
        metadata: ObjectMeta {
            name: Some(odgm_name.clone()),
            labels: odgm_labels.clone(),
            owner_references: owner_refs.clone(),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(replicas),
            selector: odgm_selector,
            template: PodTemplateSpec {
                metadata: Some(ObjectMeta {
                    labels: odgm_labels.clone(),
                    ..Default::default()
                }),
                spec: Some(PodSpec {
                    containers: vec![odgm_container],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let odgm_svc_name = format!("{}-svc", odgm_name);
    let odgm_service = Service {
        metadata: ObjectMeta {
            name: Some(odgm_svc_name),
            labels: odgm_labels.clone(),
            owner_references: owner_refs,
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: odgm_labels,
            ports: Some(vec![
                ServicePort {
                    name: Some("http".into()),
                    port: 80,
                    target_port: Some(IntOrString::Int(odgm_port)),
                    ..Default::default()
                },
                ServicePort {
                    name: Some("zenoh".into()),
                    port: 17447,
                    target_port: Some(IntOrString::Int(17447)),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        }),
        ..Default::default()
    };
    Ok((odgm_deployment, odgm_service))
}
