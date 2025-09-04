use std::sync::Arc;

use chrono::Utc;
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams};
use kube::core::DynamicObject;
use kube::discovery::ApiResource;
use kube::runtime::controller::Action;
use kube::{Client, Resource, ResourceExt};
use serde_json::json;
use tracing::{instrument, trace};

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Service;

use crate::config::CrmConfig;
use crate::crd::class_runtime::{
    ClassRuntime, Condition, ConditionStatus, ConditionType, ResourceRef,
};
use crate::nfr::parse_match_labels;
use crate::templates::{RenderContext, RenderedResource, TemplateManager};
use envconfig::Envconfig;

use super::{ControllerContext, ReconcileErr, into_internal};
use crate::controller::events::{REASON_APPLIED, emit_event};
use crate::controller::status::progressing as build_progressing_status;
use serde_json::Value as JsonValue;

const FINALIZER: &str = "oaas.io/finalizer";

fn owner_label_selector(name: &str) -> String {
    format!("oaas.io/owner={}", name)
}

fn compute_enable_odgm(
    cfg: &CrmConfig,
    spec: &crate::crd::class_runtime::ClassRuntimeSpec,
) -> bool {
    // Policy: ODGM is enabled by default. An explicit env override
    // OPRC_CRM_FEATURES_ODGM=false disables it globally. If an addons list
    // is provided it becomes an allow-list (must include "odgm" to keep it on).
    if let Some(false) = cfg.features.odgm_sidecar {
        return false; // global off switch
    }
    match spec.addons.as_ref() {
        None => true, // default-on when no explicit allow-list
        Some(list) => list.iter().any(|a| a.eq_ignore_ascii_case("odgm")),
    }
}

// moved to controller::status

fn split_api_version(api_version: &str) -> (String, String) {
    let mut parts = api_version.splitn(2, '/');
    let group = parts.next().unwrap_or("").to_string();
    let version = parts.next().unwrap_or("").to_string();
    (group, version)
}

fn dynamic_target_from(
    api_version: &str,
    kind: &str,
    manifest: &serde_json::Value,
    default_name: &str,
) -> (kube::core::GroupVersionKind, String) {
    let (group, version) = split_api_version(api_version);
    let gvk = kube::core::GroupVersionKind::gvk(&group, &version, kind);
    let name = manifest
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(default_name)
        .to_string();
    (gvk, name)
}

#[instrument(skip_all, fields(ns = %obj.namespace().unwrap_or_else(|| "default".into()), name = %obj.name_any()))]
pub async fn reconcile(
    obj: Arc<ClassRuntime>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileErr> {
    let ns = obj.namespace().unwrap_or_else(|| "default".to_string());
    let name = obj.name_any();
    let uid = obj.meta().uid.clone();

    let dr_api: Api<ClassRuntime> = Api::namespaced(ctx.client.clone(), &ns);

    // Handle delete: cleanup children then remove finalizer
    if obj.meta().deletion_timestamp.is_some() {
        delete_children(&ctx.client, &ns, &name, ctx.include_knative).await;
        ctx.dr_cache.remove(&format!("{}/{}", ns, name)).await;
        trace!(%ns, %name, "cache: removed on delete");
        if obj
            .meta()
            .finalizers
            .as_ref()
            .map(|f| f.iter().any(|x| x == FINALIZER))
            .unwrap_or(false)
        {
            let finals = obj
                .meta()
                .finalizers
                .clone()
                .unwrap_or_default()
                .into_iter()
                .filter(|f| f != FINALIZER)
                .collect::<Vec<_>>();
            let patch = json!({"metadata": {"finalizers": finals}});
            let _ = dr_api
                .patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
                .await
                .map_err(into_internal)?;
        }
        return Ok(Action::await_change());
    }

    // Ensure finalizer
    if !obj
        .meta()
        .finalizers
        .as_ref()
        .map(|f| f.iter().any(|x| x == FINALIZER))
        .unwrap_or(false)
    {
        let mut finals = obj.meta().finalizers.clone().unwrap_or_default();
        finals.push(FINALIZER.to_string());
        let patch = json!({"metadata": {"finalizers": finals}});
        let _ = dr_api
            .patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
            .await
            .map_err(into_internal)?;
    }

    // Apply workload (Deployment + Service) with SSA
    let spec = &obj.spec;
    let enable_odgm = compute_enable_odgm(&ctx.cfg, spec);
    apply_workload(
        &ctx.client,
        &ns,
        &name,
        uid.as_deref(),
        enable_odgm,
        &ctx.cfg.profile,
        spec,
        ctx.include_knative,
    )
    .await?;

    // Upsert into local cache
    ctx.dr_cache
        .upsert(format!("{}/{}", ns, name), (*obj).clone())
        .await;
    trace!(%ns, %name, "cache: upsert after apply_workload");

    // Ensure metrics scrape targets if enabled
    let monitor_refs =
        ensure_metrics_targets(&ctx, &ns, &name, ctx.include_knative)
            .await
            .unwrap_or_default();

    // Emit a basic Event for successful apply
    emit_event(
        &ctx.event_recorder,
        &ns,
        &name,
        uid.as_deref(),
        REASON_APPLIED,
        "Apply",
        Some(format!("Applied workload for {}", name)),
    )
    .await;

    // Update status (observedGeneration, phase) — only patch when it would change materially
    let now = Utc::now().to_rfc3339();
    let mut status_obj =
        build_progressing_status(now.clone(), obj.meta().generation);
    if ctx.cfg.features.prometheus.unwrap_or(false) && !ctx.metrics_enabled {
        if let Some(ref mut conds) = status_obj.conditions {
            conds.push(Condition {
                type_: ConditionType::Unknown,
                status: ConditionStatus::False,
                reason: Some("PrometheusDisabled".into()),
                message: Some(
                    "Prometheus Operator CRDs not found; metrics disabled"
                        .into(),
                ),
                last_transition_time: Some(now.clone()),
            });
        }
    }
    if !monitor_refs.is_empty() {
        status_obj.resource_refs = Some(
            monitor_refs
                .into_iter()
                .map(|(k, n)| ResourceRef { kind: k, name: n })
                .collect(),
        );
    }
    // Only write status when it actually changes (ignore timestamp-only churn)
    if should_patch_status(obj.status.as_ref(), &status_obj) {
        let status = json!({ "status": status_obj });
        let _ = dr_api
            .patch_status(
                &name,
                &PatchParams::default(),
                &Patch::Merge(&status),
            )
            .await
            .map_err(into_internal)?;
    }

    // Wait for real changes instead of periodic requeues to avoid tight loops
    Ok(Action::await_change())
}

#[instrument(skip_all, fields(ns = %ns, name = %name, knative = %include_knative))]
async fn ensure_metrics_targets(
    ctx: &ControllerContext,
    ns: &str,
    name: &str,
    include_knative: bool,
) -> Result<Vec<(String, String)>, ReconcileErr> {
    if !ctx.metrics_enabled {
        return Ok(vec![]);
    }
    let provider = ctx.metrics_provider.as_ref().unwrap();
    let penv = crate::config::PromConfig::init_from_env().ok();
    let scrape_kind = match penv.as_ref().and_then(|e| e.scrape_kind.as_deref())
    {
        Some("service") => crate::nfr::ScrapeKind::Service,
        Some("pod") => crate::nfr::ScrapeKind::Pod,
        _ => crate::nfr::ScrapeKind::Auto,
    };
    let match_labels = penv
        .and_then(|e| e.match_labels.as_ref().map(|s| parse_match_labels(s)));
    let refs = provider
        .ensure_targets(
            ns,
            name,
            name,
            "oaas.io/owner",
            scrape_kind,
            &match_labels,
            include_knative,
        )
        .await
        .map_err(|e| ReconcileErr::Internal(e.to_string()))?;
    Ok(refs)
}

#[instrument(skip_all, fields(ns = %ns, name = %name, knative = %include_knative))]
async fn apply_workload(
    client: &Client,
    ns: &str,
    name: &str,
    owner_uid: Option<&str>,
    enable_odgm_sidecar: bool,
    profile: &str,
    spec: &crate::crd::class_runtime::ClassRuntimeSpec,
    include_knative: bool,
) -> Result<(), ReconcileErr> {
    let tm = TemplateManager::new(include_knative);
    let resources = tm
        .render_workload(RenderContext {
            name,
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid,
            enable_odgm_sidecar,
            profile,
            spec,
        })
        .map_err(|e| ReconcileErr::Internal(e.to_string()))?;
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let pp = PatchParams::apply("oprc-crm").force();

    for rr in resources {
        match rr {
            RenderedResource::Deployment(mut dep) => {
                let suppress_replicas = spec
                    .nfr
                    .as_ref()
                    .and_then(|n| n.enforcement.as_ref())
                    .map(|e| {
                        e.mode
                            .as_deref()
                            .unwrap_or("observe")
                            .eq_ignore_ascii_case("enforce")
                            && e.dimensions
                                .as_ref()
                                .map(|ds| ds.iter().any(|d| d == "replicas"))
                                .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if suppress_replicas {
                    if let Some(ref mut spec) = dep.spec {
                        spec.replicas = None;
                    }
                }
                let dep_name = dep
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| name.to_string());
                let dep_json =
                    serde_json::to_value(&dep).map_err(into_internal)?;
                let _ = dep_api
                    .patch(&dep_name, &pp, &Patch::Apply(&dep_json))
                    .await
                    .map_err(into_internal)?;
            }
            RenderedResource::Service(svc) => {
                let svc_name = svc
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}-svc", name));
                let svc_json =
                    serde_json::to_value(&svc).map_err(into_internal)?;
                let _ = svc_api
                    .patch(&svc_name, &pp, &Patch::Apply(&svc_json))
                    .await
                    .map_err(into_internal)?;
            }
            RenderedResource::Other {
                api_version,
                kind,
                manifest,
            } => {
                let (gvk, name_dyn) =
                    dynamic_target_from(&api_version, &kind, &manifest, name);
                let ar = ApiResource::from_gvk(&gvk);
                let dyn_api: Api<DynamicObject> =
                    Api::namespaced_with(client.clone(), ns, &ar);
                let pp_dyn = PatchParams::apply("oprc-crm").force();
                let _ = dyn_api
                    .patch(&name_dyn, &pp_dyn, &Patch::Apply(&manifest))
                    .await
                    .map_err(into_internal)?;
            }
        }
    }

    Ok(())
}

/// Compare two status objects for material differences, ignoring timestamp-only fields
/// that would otherwise cause infinite reconcile loops (last_updated/lastTransitionTime).
fn should_patch_status(
    current: Option<&crate::crd::class_runtime::ClassRuntimeStatus>,
    desired: &crate::crd::class_runtime::ClassRuntimeStatus,
) -> bool {
    match current {
        None => true,
        Some(cur) => normalize_status(cur) != normalize_status(desired),
    }
}

fn normalize_status(
    s: &crate::crd::class_runtime::ClassRuntimeStatus,
) -> JsonValue {
    let mut v = serde_json::to_value(s).unwrap_or_else(|_| json!({}));
    if let JsonValue::Object(ref mut map) = v {
        // Drop volatile top-level timestamp
        map.remove("last_updated");
        // Normalize conditions: drop lastTransitionTime from each condition
        if let Some(JsonValue::Array(conds)) = map.get_mut("conditions") {
            for c in conds.iter_mut() {
                if let Some(obj) = c.as_object_mut() {
                    obj.remove("lastTransitionTime");
                }
            }
        }
        // Normalize resource_refs order to ensure stable equality when content is identical
        if let Some(JsonValue::Array(refs)) = map.get_mut("resource_refs") {
            refs.sort_by(|a, b| {
                let ak = a
                    .as_object()
                    .and_then(|o| o.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let an = a
                    .as_object()
                    .and_then(|o| o.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bk = b
                    .as_object()
                    .and_then(|o| o.get("kind"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let bn = b
                    .as_object()
                    .and_then(|o| o.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                (ak, an).cmp(&(bk, bn))
            });
        }
    }
    v
}

#[instrument(skip_all, fields(ns = %ns, name = %name, knative = %include_knative))]
async fn delete_children(
    client: &Client,
    ns: &str,
    name: &str,
    include_knative: bool,
) {
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&owner_label_selector(name));

    if let Ok(list) = dep_api.list(&lp).await {
        for d in list {
            let n = d.name_any();
            let _ = dep_api.delete(&n, &DeleteParams::default()).await;
        }
    }
    if let Ok(list) = svc_api.list(&lp).await {
        for s in list {
            let n = s.name_any();
            let _ = svc_api.delete(&n, &DeleteParams::default()).await;
        }
    }

    if include_knative {
        if let Ok(discovery) =
            kube::discovery::Discovery::new(client.clone()).run().await
        {
            if discovery
                .groups()
                .any(|g| g.name() == "serving.knative.dev")
            {
                let ar =
                    ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                        "serving.knative.dev",
                        "v1",
                        "Service",
                    ));
                let dyn_api: Api<DynamicObject> =
                    Api::namespaced_with(client.clone(), ns, &ar);
                let _ = dyn_api.delete(name, &DeleteParams::default()).await;
            }
        }
    }
}

// Intentionally no re-exports here; unit tests for helpers live with their modules.
