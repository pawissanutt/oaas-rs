use std::sync::Arc;
use tokio::time::Duration;

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
use crate::crd::deployment_record::{
    Condition, ConditionStatus, ConditionType, DeploymentRecord,
    DeploymentRecordStatus, ResourceRef,
};
use crate::templates::{RenderContext, RenderedResource, TemplateManager};
use crate::nfr::parse_match_labels;
use envconfig::Envconfig;

use super::{build_obj_ref, into_internal, ControllerContext, ReconcileErr};

const FINALIZER: &str = "oaas.io/finalizer";

fn owner_label_selector(name: &str) -> String {
    format!("oaas.io/owner={}", name)
}

fn compute_enable_odgm(
    cfg: &CrmConfig,
    spec: &crate::crd::deployment_record::DeploymentRecordSpec,
) -> bool {
    let feature_odgm = cfg.features.odgm_sidecar.unwrap_or(false);
    let addons = spec
        .addons
        .as_ref()
        .map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    feature_odgm && addons.iter().any(|a| *a == "odgm")
}

fn build_progressing_status(
    now: String,
    generation: Option<i64>,
) -> DeploymentRecordStatus {
    DeploymentRecordStatus {
        phase: Some("Progressing".to_string()),
        message: Some("Applied workload".to_string()),
        observed_generation: generation,
        last_updated: Some(now.clone()),
        conditions: Some(vec![Condition {
            type_: ConditionType::Progressing,
            status: ConditionStatus::True,
            reason: Some("ApplySuccessful".into()),
            message: Some("Resources applied".into()),
            last_transition_time: Some(now),
        }]),
        resource_refs: None,
        nfr_recommendations: None,
        last_applied_recommendations: None,
        last_applied_at: None,
    }
}

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
    obj: Arc<DeploymentRecord>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileErr> {
    let ns = obj.namespace().unwrap_or_else(|| "default".to_string());
    let name = obj.name_any();
    let uid = obj.meta().uid.clone();

    let dr_api: Api<DeploymentRecord> = Api::namespaced(ctx.client.clone(), &ns);

    // Handle delete: cleanup children then remove finalizer
    if obj.meta().deletion_timestamp.is_some() {
        delete_children(&ctx.client, &ns, &name, ctx.include_knative).await;
        {
            let mut w = ctx.dr_cache.write().await;
            w.remove(&format!("{}/{}", ns, name));
        }
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
    {
        let mut w = ctx.dr_cache.write().await;
        w.insert(format!("{}/{}", ns, name), (*obj).clone());
    }
    trace!(%ns, %name, "cache: upsert after apply_workload");

    // Ensure metrics scrape targets if enabled
    let monitor_refs = ensure_metrics_targets(&ctx, &ns, &name, ctx.include_knative)
        .await
        .unwrap_or_default();

    // Emit a basic Event for successful apply
    let _ = ctx
        .event_recorder
        .publish(
            &kube::runtime::events::Event {
                type_: kube::runtime::events::EventType::Normal,
                reason: "Applied".into(),
                note: Some(format!("Applied workload for {}", name)),
                action: "Apply".into(),
                secondary: None,
            },
            &build_obj_ref(&ns, &name, uid.as_deref()),
        )
        .await;

    // Update status (observedGeneration, phase)
    let now = Utc::now().to_rfc3339();
    let mut status_obj = build_progressing_status(now.clone(), obj.meta().generation);
    if ctx.cfg.features.prometheus.unwrap_or(false) && !ctx.metrics_enabled {
        if let Some(ref mut conds) = status_obj.conditions {
            conds.push(Condition {
                type_: ConditionType::Unknown,
                status: ConditionStatus::False,
                reason: Some("PrometheusDisabled".into()),
                message: Some(
                    "Prometheus Operator CRDs not found; metrics disabled".into(),
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
    let status = json!({ "status": status_obj });
    let _ = dr_api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&status))
        .await
        .map_err(into_internal)?;

    Ok(Action::requeue(Duration::from_secs(60)))
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
    let scrape_kind = match penv.as_ref().and_then(|e| e.scrape_kind.as_deref()) {
        Some("service") => crate::nfr::ScrapeKind::Service,
        Some("pod") => crate::nfr::ScrapeKind::Pod,
        _ => crate::nfr::ScrapeKind::Auto,
    };
    let match_labels = penv.and_then(|e| e.match_labels.as_ref().map(|s| parse_match_labels(s)));
    let refs = provider
        .ensure_targets(ns, name, name, "oaas.io/owner", scrape_kind, &match_labels, include_knative)
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
    spec: &crate::crd::deployment_record::DeploymentRecordSpec,
    include_knative: bool,
) -> Result<(), ReconcileErr> {
    let tm = TemplateManager::new(include_knative);
    let resources = tm.render_workload(RenderContext {
        name,
        owner_api_version: "oaas.io/v1alpha1",
        owner_kind: "DeploymentRecord",
        owner_uid,
        enable_odgm_sidecar,
        profile,
        spec,
    });
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
                            && e
                                .dimensions
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
                let dep_name = dep.metadata.name.clone().unwrap_or_else(|| name.to_string());
                let dep_json = serde_json::to_value(&dep).map_err(into_internal)?;
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
                let svc_json = serde_json::to_value(&svc).map_err(into_internal)?;
                let _ = svc_api
                    .patch(&svc_name, &pp, &Patch::Apply(&svc_json))
                    .await
                    .map_err(into_internal)?;
            }
            RenderedResource::Other { api_version, kind, manifest } => {
                let (gvk, name_dyn) = dynamic_target_from(&api_version, &kind, &manifest, name);
                let ar = ApiResource::from_gvk(&gvk);
                let dyn_api: Api<DynamicObject> = Api::namespaced_with(client.clone(), ns, &ar);
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
        if let Ok(discovery) = kube::discovery::Discovery::new(client.clone()).run().await {
            if discovery.groups().any(|g| g.name() == "serving.knative.dev") {
                let ar = ApiResource::from_gvk(&kube::core::GroupVersionKind::gvk(
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
