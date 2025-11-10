use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams};
use kube::core::DynamicObject;
use kube::discovery::ApiResource;
use kube::runtime::controller::Action;
use kube::{Client, Resource, ResourceExt};
use serde_json::json;
use tracing::{debug, info, instrument, trace};

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
use crate::controller::fsm::{
    descriptors_builtin,
    evaluator::{EvalInput, Phase, evaluate},
    observed_lister,
};
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
    if !cfg.features.odgm_sidecar {
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
        info!(%ns, %name, "reconcile: deletion timestamp detected; starting child cleanup");
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
            info!(%ns, %name, "reconcile: removing finalizer");
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
        info!(%ns, %name, "reconcile: adding finalizer");
        let mut finals = obj.meta().finalizers.clone().unwrap_or_default();
        finals.push(FINALIZER.to_string());
        let patch = json!({"metadata": {"finalizers": finals}});
        let _ = dr_api
            .patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
            .await
            .map_err(into_internal)?;
    }

    // FSM path (feature flagged): observe children first, then decide apply vs progress.
    let fsm_enabled = ctx.cfg.features.fsm;

    // Apply workload (Deployment + Service) with SSA (legacy or when evaluator signals Apply)
    let spec = &obj.spec;
    let enable_odgm = compute_enable_odgm(&ctx.cfg, spec);
    info!(%ns, %name, enable_odgm, profile=%ctx.cfg.profile, "reconcile: begin apply_workload");
    let mut phase: Phase = Phase::Pending;
    let mut next_action = Action::await_change();
    if fsm_enabled {
        let observed = observed_lister::observe_children(
            ctx.client.clone(),
            &ns,
            &name,
            ctx.include_knative,
        )
        .await;
        // choose descriptor based on include_knative flag
        let desc = if ctx.include_knative {
            descriptors_builtin::descriptor_knative()
        } else {
            descriptors_builtin::descriptor_k8s()
        };
        // Phases in status are written in lowercase; accept lowercase and also fallback to case-insensitive match.
        let prev_phase = obj
            .status
            .as_ref()
            .and_then(|s| s.phase.clone())
            .and_then(|p| match p.to_ascii_lowercase().as_str() {
                "pending" => Some(Phase::Pending),
                "applying" => Some(Phase::Applying),
                "progressing" => Some(Phase::Progressing),
                "available" => Some(Phase::Available),
                "degraded" => Some(Phase::Degraded),
                "deleting" => Some(Phase::Deleting),
                _ => None,
            });
        let observed_generation =
            obj.status.as_ref().and_then(|s| s.observed_generation);
        // Default to "no change" when observedGeneration is absent; brand new resources
        // will still trigger an apply because they have no children yet.
        let generation_changed = observed_generation
            .map(|g| g != obj.meta().generation.unwrap_or(0))
            .unwrap_or(false);
        let eval = evaluate(EvalInput {
            phase: prev_phase,
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed,
            class_name: &name,
            functions_total: spec.functions.len(),
        });
        phase = eval.phase;
        if eval.actions.iter().any(|a| {
            matches!(
                a,
                crate::controller::fsm::actions::FsmAction::ApplyWorkload
            )
        }) {
            apply_workload(
                &ctx.client,
                &ns,
                &name,
                uid.as_deref(),
                enable_odgm,
                &ctx.cfg.profile,
                ctx.cfg.templates.odgm_img_override.as_deref(),
                spec,
                ctx.include_knative,
            )
            .await?;
            info!(%ns, %name, "reconcile(fsm): apply_workload complete");
        }
        // Execute orphan deletions if requested
        for act in eval.actions.iter() {
            if let crate::controller::fsm::actions::FsmAction::DeleteOrphans(
                list,
            ) = act
            {
                if !list.is_empty() {
                    info!(%ns, %name, count=list.len(), "fsm: deleting orphan children");
                    // Best-effort deletes across known child kinds
                    let dep_api: Api<Deployment> =
                        Api::namespaced(ctx.client.clone(), &ns);
                    let svc_api: Api<Service> =
                        Api::namespaced(ctx.client.clone(), &ns);
                    for orphan in list {
                        let _ = dep_api
                            .delete(orphan, &kube::api::DeleteParams::default())
                            .await;
                        let _ = svc_api
                            .delete(orphan, &kube::api::DeleteParams::default())
                            .await;
                        if ctx.include_knative {
                            let ar = ApiResource::from_gvk(
                                &kube::core::GroupVersionKind::gvk(
                                    "serving.knative.dev",
                                    "v1",
                                    "Service",
                                ),
                            );
                            let dyn_api: Api<DynamicObject> =
                                Api::namespaced_with(
                                    ctx.client.clone(),
                                    &ns,
                                    &ar,
                                );
                            let _ = dyn_api
                                .delete(
                                    orphan,
                                    &kube::api::DeleteParams::default(),
                                )
                                .await;
                        }
                    }
                }
            }
        }
        match phase {
            Phase::Available | Phase::Deleting => {}
            _ => {
                next_action = Action::requeue(Duration::from_secs(5));
            }
        }
    } else {
        apply_workload(
            &ctx.client,
            &ns,
            &name,
            uid.as_deref(),
            enable_odgm,
            &ctx.cfg.profile,
            ctx.cfg.templates.odgm_img_override.as_deref(),
            spec,
            ctx.include_knative,
        )
        .await?;
        info!(%ns, %name, "reconcile: apply_workload complete");
        next_action = Action::requeue(Duration::from_secs(5));
    }

    // Upsert into local cache
    ctx.dr_cache
        .upsert(format!("{}/{}", ns, name), (*obj).clone())
        .await;
    trace!(%ns, %name, "cache: upsert after apply_workload");
    info!(%ns, %name, "reconcile: cached object state updated");

    // Ensure metrics scrape targets if enabled
    let monitor_refs =
        ensure_metrics_targets(&ctx, &ns, &name, ctx.include_knative)
            .await
            .unwrap_or_default();
    info!(%ns, %name, count=%monitor_refs.len(), "reconcile: metrics targets ensured");

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
    let selected_template = spec
        .selected_template
        .clone()
        .unwrap_or_else(|| ctx.cfg.profile.clone());
    let mut status_obj = build_progressing_status(
        now.clone(),
        obj.meta().generation,
        spec,
        &name,
        &ns,
        &selected_template,
    );
    if fsm_enabled {
        status_obj.phase = Some(match phase {
            Phase::Pending => "pending".into(),
            Phase::Applying => "applying".into(),
            Phase::Progressing => "progressing".into(),
            Phase::Available => "available".into(),
            Phase::Degraded => "degraded".into(),
            Phase::Deleting => "deleting".into(),
        });
        // Replace conditions with FSM-driven ones while preserving NfrObserved (conditions builder handles preservation)
        status_obj.conditions =
            Some(crate::controller::fsm::conditions::build_conditions(
                phase,
                obj.status.as_ref().and_then(|s| s.conditions.as_ref()),
            ));
        // Update per-function readiness if predicted list exists and spec defines functions.
        if spec.functions.len() > 0 {
            if let Some(ref mut funcs) = status_obj.functions {
                // We need the observed children we already built in FSM path; recompute here to avoid threading.
                let observed =
                    crate::controller::fsm::observed_lister::observe_children(
                        ctx.client.clone(),
                        &ns,
                        &name,
                        ctx.include_knative,
                    )
                    .await;
                crate::controller::fsm::function_readiness::update_function_readiness(funcs, &observed);
            }
        }
    }
    if ctx.cfg.features.prometheus && !ctx.metrics_enabled {
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
    // Discover router services for status reporting (observed state)
    {
        let svc_api: Api<Service> = Api::namespaced(ctx.client.clone(), &ns);
        if let Ok(list) = svc_api
            .list(&ListParams::default().labels("app=router,platform=oaas"))
            .await
        {
            let mut routers: Vec<crate::crd::class_runtime::RouterEndpoint> =
                Vec::new();
            for svc in list.items.into_iter() {
                let mut port_i32: i32 = 17447;
                if let Some(ports) =
                    svc.spec.as_ref().and_then(|s| s.ports.as_ref())
                {
                    if let Some(p) = ports
                        .iter()
                        .find(|p| p.name.as_deref() == Some("zenoh"))
                    {
                        port_i32 = p.port as i32;
                    } else if let Some(p) = ports.first() {
                        port_i32 = p.port as i32;
                    }
                }
                if let Some(svc_name) = svc.metadata.name.clone() {
                    routers.push(crate::crd::class_runtime::RouterEndpoint {
                        service: svc_name,
                        port: port_i32,
                    });
                }
            }
            if !routers.is_empty() {
                status_obj.routers = Some(routers);
            }
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
    // Merge with existing status to avoid clobbering analyzer/enforcer fields
    let merged_status = crate::controller::status_reducer::merge_status(
        obj.status.as_ref(),
        status_obj,
    );
    // Only write status when it actually changes (ignore timestamp-only churn)
    if should_patch_status(obj.status.as_ref(), &merged_status) {
        trace!(%ns, %name, "reconcile: status changed; patching status");
        let status = json!({ "status": merged_status });
        let _ = dr_api
            .patch_status(
                &name,
                &PatchParams::default(),
                &Patch::Merge(&status),
            )
            .await
            .map_err(into_internal)?;
    } else {
        trace!(%ns, %name, "reconcile: status unchanged; skipping patch");
    }

    // Wait for real changes instead of periodic requeues to avoid tight loops
    Ok(next_action)
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
    odgm_image_override: Option<&str>,
    spec: &crate::crd::class_runtime::ClassRuntimeSpec,
    include_knative: bool,
) -> Result<(), ReconcileErr> {
    // Discover in-namespace Zenoh router Service (deployed by Helm chart) by labels
    // app=router, platform=oaas. This allows templates to wire OPRC_ZENOH_* envs.
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let mut router_service_name: Option<String> = None;
    let mut router_service_port: Option<i32> = None;
    if let Ok(list) = svc_api
        .list(&ListParams::default().labels("app=router,platform=oaas"))
        .await
    {
        if let Some(svc) = list.items.into_iter().next() {
            router_service_name = svc.metadata.name.clone();
            // pick first port or a port named "zenoh"; default 17447 when absent
            if let Some(ports) =
                svc.spec.as_ref().and_then(|s| s.ports.as_ref())
            {
                // try named "zenoh" first
                if let Some(p) =
                    ports.iter().find(|p| p.name.as_deref() == Some("zenoh"))
                {
                    router_service_port = Some(p.port as i32);
                } else if let Some(p) = ports.first() {
                    router_service_port = Some(p.port as i32);
                }
            }
            if router_service_port.is_none() {
                router_service_port = Some(17447);
            }
        }
    }
    let tm = TemplateManager::new(include_knative);
    let resources = tm
        .render_workload(RenderContext {
            name,
            namespace: &ns,
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid,
            enable_odgm_sidecar,
            profile,
            odgm_image_override,
            router_service_name,
            router_service_port,
            spec,
        })
        .map_err(|e| ReconcileErr::Internal(e.to_string()))?;
    info!(%ns, %name, total=resources.len(), include_knative, "apply_workload: rendering complete; applying resources");
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let pp = PatchParams::apply("oprc-crm").force();

    for rr in resources {
        match rr {
            RenderedResource::Deployment(mut dep) => {
                let dep_name_log = dep
                    .metadata
                    .name
                    .clone()
                    .unwrap_or_else(|| name.to_string());
                info!(%ns, %name, kind="Deployment", resource=%dep_name_log, "apply_workload: applying resource");
                let suppress_replicas = spec
                    .enforcement
                    .as_ref()
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
                        trace!(%ns, %name, "apply_workload: replicas suppressed under enforcement");
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
                info!(%ns, %name, kind="Service", resource=%svc_name, "apply_workload: applying resource");
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
                info!(%ns, %name, %kind, %api_version, "apply_workload: applying dynamic resource");
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
        None => {
            debug!("should_patch_status: no current status, patching");
            true
        }
        Some(cur) => {
            let cur_norm = normalize_status(cur);
            let des_norm = normalize_status(desired);
            let differs = cur_norm != des_norm;
            if differs {
                debug!(
                    "should_patch_status: status differs, patching\ncurrent={}\ndesired={}",
                    serde_json::to_string(&cur_norm).unwrap_or_default(),
                    serde_json::to_string(&des_norm).unwrap_or_default()
                );
            } else {
                trace!("should_patch_status: status identical, skipping patch");
            }
            differs
        }
    }
}

fn normalize_status(
    s: &crate::crd::class_runtime::ClassRuntimeStatus,
) -> JsonValue {
    let mut v = serde_json::to_value(s).unwrap_or_else(|_| json!({}));
    if let JsonValue::Object(ref mut map) = v {
        // Drop volatile fields that change every reconcile without semantic meaning
        map.remove("last_updated");
        map.remove("observed_generation");
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
    info!(%ns, %name, "delete_children: starting child resource deletion");
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&owner_label_selector(name));

    if let Ok(list) = dep_api.list(&lp).await {
        info!(%ns, %name, count=list.items.len(), "delete_children: deleting deployments");
        for d in list {
            let n = d.name_any();
            let _ = dep_api.delete(&n, &DeleteParams::default()).await;
        }
    }
    if let Ok(list) = svc_api.list(&lp).await {
        info!(%ns, %name, count=list.items.len(), "delete_children: deleting services");
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
                info!(%ns, %name, "delete_children: deleting knative Services by label");
                let ar =
                    ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
                        "serving.knative.dev",
                        "v1",
                        "Service",
                    ));
                let dyn_api: Api<DynamicObject> =
                    Api::namespaced_with(client.clone(), ns, &ar);
                let lp =
                    ListParams::default().labels(&owner_label_selector(name));
                if let Ok(list) = dyn_api.list(&lp).await {
                    for svc in list {
                        let n = svc.name_any();
                        let _ =
                            dyn_api.delete(&n, &DeleteParams::default()).await;
                    }
                } else {
                    // Fallback: attempt delete by base name to preserve prior behavior
                    let _ =
                        dyn_api.delete(name, &DeleteParams::default()).await;
                }
            }
        }
    }
    info!(%ns, %name, "delete_children: completed child resource deletion");
}

// Intentionally no re-exports here; unit tests for helpers live with their modules.
