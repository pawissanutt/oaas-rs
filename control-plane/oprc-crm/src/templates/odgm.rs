use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::EnvVar;
use k8s_openapi::api::core::v1::{
    Container, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    LabelSelector, ObjectMeta, OwnerReference,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

use crate::crd::class_runtime::{
    ClassRuntimeSpec as DeploymentRecordSpec, InvocationsSpec,
};
use crate::templates::manager::TemplateError;
use oprc_grpc::CreateCollectionRequest;

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
    merged_invocations: Option<&InvocationsSpec>,
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
        .unwrap_or("none");
    // Capture references to invocations and options if present
    // Use merged invocations (predicted + user) when provided, else fall back to spec provided
    let invocations = merged_invocations.or_else(|| {
        spec.odgm_config
            .as_ref()
            .and_then(|c| c.invocations.as_ref())
    });
    let options = spec.odgm_config.as_ref().and_then(|c| c.options.as_ref());

    names
        .iter()
        .map(|n| {
            let assigns = spec
                .odgm_config
                .as_ref()
                .and_then(|c| c.collection_assignments.get(n))
                .map(|v| v.as_slice());
            build_collection_request(
                n,
                partition_count,
                replica_count,
                shard_type,
                invocations,
                options,
                assigns,
            )
        })
        .collect()
}

pub fn collections_env_var_ctx(
    ctx: &crate::templates::manager::RenderContext<'_>,
    names: &Vec<String>,
) -> Result<EnvVar, TemplateError> {
    let merged_inv = merged_function_routes(ctx);
    let reqs = build_requests(ctx.spec, names, merged_inv.as_ref());
    let value = serde_json::to_string(&reqs)?;
    Ok(EnvVar {
        name: "ODGM_COLLECTION".to_string(),
        value: Some(value),
        ..Default::default()
    })
}

pub fn collections_env_json_ctx(
    ctx: &crate::templates::manager::RenderContext<'_>,
    names: &Vec<String>,
) -> serde_json::Value {
    let merged_inv = merged_function_routes(ctx);
    let reqs = build_requests(ctx.spec, names, merged_inv.as_ref());
    serde_json::json!({ "name": "ODGM_COLLECTION", "value": serde_json::to_string(&reqs).unwrap(), })
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

    // When collections are configured, also pass ODGM_COLLECTION (and ODGM_LOG if set)
    if let Some(cols) = collection_names(ctx.spec) {
        env.push(collections_env_var_ctx(ctx, cols)?);
        if let Some(e) = log_env_var(ctx.spec) {
            env.push(e);
        }
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
    // Inject ODGM_NODE_ID and ODGM_MEMBERS if provided via ODGM config
    if let Some(cfg) = ctx.spec.odgm_config.as_ref() {
        let mut env = odgm_container.env.take().unwrap_or_default();
        env.push(EnvVar {
            name: "ODGM_NODE_ID".into(),
            value: cfg.node_id.map(|v| v.to_string()),
            ..Default::default()
        });
        // Build members list from env_node_ids mapping values flattened & sorted unique
        let mut members: Vec<u64> = cfg
            .env_node_ids
            .values()
            .flat_map(|v| v.iter().copied())
            .collect();
        members.sort_unstable();
        members.dedup();
        let members_str = members
            .into_iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",");
        env.push(EnvVar {
            name: "ODGM_MEMBERS".into(),
            value: Some(members_str),
            ..Default::default()
        });
        // Reassign mutated env so subsequent blocks (collections) don't lose these entries
        odgm_container.env = Some(env);
    }
    if let Some(cols) = collection_names(ctx.spec) {
        let mut env = odgm_container.env.take().unwrap_or_default();
        env.push(collections_env_var_ctx(ctx, cols)?);
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

/// Merge predicted function routes with user-provided invocations (if any) and return
/// a synthesized InvocationsSpec. Returns None if there are no functions (nothing to route).
fn merged_function_routes(
    ctx: &crate::templates::manager::RenderContext<'_>,
) -> Option<InvocationsSpec> {
    if ctx.spec.functions.is_empty() {
        tracing::trace!(name=%ctx.name, "merged_function_routes: no functions");
        return None;
    }
    // Predicted base map
    let predicted =
        crate::templates::manager::TemplateManager::predicted_function_routes(
            ctx,
        );
    let predicted_keys: Vec<String> = predicted.keys().cloned().collect();
    // If user supplied invocations with routes, enrich only existing keys without adding new ones.
    let merged = if let Some(cfg) = ctx.spec.odgm_config.as_ref() {
        if let Some(inv) = cfg.invocations.as_ref() {
            if !inv.fn_routes.is_empty() {
                // Build a lookup from function_key -> predicted route for enrichment
                let mut fk_index: std::collections::BTreeMap<
                    String,
                    crate::crd::class_runtime::FunctionRoute,
                > = std::collections::BTreeMap::new();
                for (_k, v) in &predicted {
                    if let Some(ref fk) = v.function_key {
                        fk_index.insert(fk.clone(), v.clone());
                    }
                }
                // Enrich user-provided keys: if url empty, try to fill from predicted via function_key
                let mut fn_routes = inv.fn_routes.clone();
                for (_k, v) in fn_routes.iter_mut() {
                    if v.url.is_empty() {
                        if let Some(ref fk) = v.function_key {
                            if let Some(pred) = fk_index.get(fk) {
                                v.url = pred.url.clone();
                                if v.stateless.is_none() {
                                    v.stateless = pred.stateless;
                                }
                                if v.standby.is_none() {
                                    v.standby = pred.standby;
                                }
                                if v.active_group.is_empty() {
                                    v.active_group = pred.active_group.clone();
                                }
                            }
                        }
                    }
                }
                // Also add any predicted keys that are missing
                for (k, v) in predicted.into_iter() {
                    fn_routes.entry(k).or_insert(v);
                }
                let final_keys: Vec<String> =
                    fn_routes.keys().cloned().collect();
                tracing::debug!(name=%ctx.name, predicted=?predicted_keys, user=?inv.fn_routes.keys().collect::<Vec<_>>(), merged=?final_keys, "merged_function_routes: enriched user routes");
                InvocationsSpec {
                    fn_routes,
                    disabled_fn: inv.disabled_fn.clone(),
                }
            } else {
                tracing::debug!(name=%ctx.name, predicted=?predicted_keys, "merged_function_routes: user invocations present but empty fn_routes -> using predicted only");
                InvocationsSpec {
                    fn_routes: predicted,
                    disabled_fn: Vec::new(),
                }
            }
        } else {
            tracing::debug!(name=%ctx.name, predicted=?predicted_keys, "merged_function_routes: no user invocations -> using predicted only");
            InvocationsSpec {
                fn_routes: predicted,
                disabled_fn: Vec::new(),
            }
        }
    } else {
        tracing::debug!(name=%ctx.name, predicted=?predicted_keys, "merged_function_routes: no odgm_config -> using predicted only");
        InvocationsSpec {
            fn_routes: predicted,
            disabled_fn: Vec::new(),
        }
    };
    tracing::trace!(name=%ctx.name, final_keys=?merged.fn_routes.keys().collect::<Vec<_>>(), "merged_function_routes: completed");
    Some(merged)
}
