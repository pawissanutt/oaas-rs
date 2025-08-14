use envconfig::Envconfig;
use std::sync::Arc;

use futures_util::StreamExt;
use k8s_openapi::api::{apps::v1::Deployment, core::v1::Service};
use kube::{
    Client, Resource, ResourceExt,
    api::{Api, DeleteParams, Patch, PatchParams},
    core::DynamicObject,
    discovery::ApiResource,
    runtime::{Controller, controller::Action, watcher::Config},
};
use serde_json::json;
use tokio::time::Duration;
use tracing::{error, info, instrument};

use crate::config::CrmConfig;
use crate::crd::deployment_record::{
    Condition, ConditionStatus, ConditionType, DeploymentRecord,
    DeploymentRecordStatus,
};
use crate::nfr::{
    Analyzer, PromOperatorProvider, ScrapeKind, parse_match_labels,
};
use crate::templates::{RenderContext, RenderedResource, TemplateManager};
use chrono::Utc;

#[derive(thiserror::Error, Debug)]
pub enum ReconcileErr {
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Clone)]
pub struct ControllerContext {
    pub client: Client,
    pub cfg: CrmConfig,
    pub include_knative: bool,
    pub metrics_enabled: bool,
    pub metrics_provider: Option<PromOperatorProvider>,
    pub analyzer: Analyzer,
}

pub async fn run_controller(client: Client) -> anyhow::Result<()> {
    let api: Api<DeploymentRecord> = Api::all(client.clone());
    // Load config to honor feature flags during reconcile
    let cfg = CrmConfig::init_from_env()?.apply_profile_defaults();
    // Detect Knative availability via CRD discovery
    let mut have_knative = false;
    if cfg.features.knative.unwrap_or(false) {
        if let Ok(discovery) =
            kube::discovery::Discovery::new(client.clone()).run().await
        {
            have_knative = discovery
                .groups()
                .any(|g| g.name() == "serving.knative.dev");
        }
    }
    let include_knative = have_knative && cfg.features.knative.unwrap_or(false);

    // Detect Prometheus Operator presence; auto-disable metrics if missing
    let prom_feature = cfg.features.prometheus.unwrap_or(false);
    let prom_provider = PromOperatorProvider::new(client.clone());
    let have_prom = if prom_feature {
        prom_provider.operator_crds_present().await
    } else {
        false
    };
    let metrics_enabled = prom_feature && have_prom;
    if prom_feature && !have_prom {
        tracing::warn!(
            "Prometheus Operator CRDs not found; disabling metrics features"
        );
    }
    let ctx = Arc::new(ControllerContext {
        client: client.clone(),
        cfg,
        include_knative,
        metrics_enabled,
        metrics_provider: if metrics_enabled {
            Some(prom_provider)
        } else {
            None
        },
        analyzer: Analyzer::new(),
    });

    // Spawn analyzer loop to observe metrics and update status periodically
    if ctx.metrics_enabled {
        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            analyzer_loop(ctx_clone).await;
        });
    }

    Controller::new(api, Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok((_obj_ref, action)) => {
                    info!("reconciled: requeue={:?}", action)
                }
                Err(e) => error!(error = ?e, "reconcile error"),
            }
        })
        .await;

    Ok(())
}

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
async fn reconcile(
    obj: Arc<DeploymentRecord>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileErr> {
    let ns = obj.namespace().unwrap_or_else(|| "default".to_string());
    let name = obj.name_any();
    let uid = obj.meta().uid.clone();

    let dr_api: Api<DeploymentRecord> =
        Api::namespaced(ctx.client.clone(), &ns);

    // Handle delete: cleanup children then remove finalizer
    if obj.meta().deletion_timestamp.is_some() {
        // Best-effort cleanup of children then remove our finalizer
        delete_children(&ctx.client, &ns, &name, ctx.include_knative).await;
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
    // Enable ODGM if feature flag is on and CRD requests addon "odgm"
    let spec = &obj.spec;
    let enable_odgm = compute_enable_odgm(&ctx.cfg, spec);
    // function image/port are read inside TemplateManager during rendering
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

    // Ensure metrics scrape targets if enabled
    let monitor_refs =
        ensure_metrics_targets(&ctx, &ns, &name, ctx.include_knative)
            .await
            .unwrap_or_default();
    // Analyzer runs in its own loop; don't compute recommendations here
    let recommendations: Vec<crate::crd::deployment_record::NfrRecommendation> = vec![];

    // Events pending: will emit once Recorder wiring is finalized

    // Update status (observedGeneration, phase)
    let now = Utc::now().to_rfc3339();
    let mut status_obj =
        build_progressing_status(now.clone(), obj.meta().generation);
    if ctx.cfg.features.prometheus.unwrap_or(false) && !ctx.metrics_enabled {
        // Add a PrometheusDisabled condition note
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
                .map(|(k, n)| crate::crd::deployment_record::ResourceRef {
                    kind: k,
                    name: n,
                })
                .collect(),
        );
    }
    if !recommendations.is_empty() {
        status_obj.nfr_recommendations = Some(recommendations);
    }
    let status = json!({ "status": status_obj });
    let _ = dr_api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&status))
        .await
        .map_err(into_internal)?;

    Ok(Action::requeue(Duration::from_secs(60)))
}

async fn analyzer_loop(ctx: Arc<ControllerContext>) {
    let dr_api_all: Api<DeploymentRecord> = Api::all(ctx.client.clone());
    loop {
        if !ctx.metrics_enabled {
            tokio::time::sleep(Duration::from_secs(
                ctx.cfg.analyzer_interval_secs,
            ))
            .await;
            continue;
        }
        match dr_api_all.list(&kube::api::ListParams::default()).await {
            Ok(list) => {
                for dr in list.items {
                    if dr.metadata.deletion_timestamp.is_some() {
                        continue;
                    }
                    let ns = match dr.namespace() {
                        Some(n) => n,
                        None => continue,
                    };
                    let name = dr.name_any();
                    match ctx.analyzer.observe_only(&ns, &name).await {
                        Ok(recs) => {
                            // Merge NfrObserved into existing conditions
                            let mut conds = dr
                                .status
                                .as_ref()
                                .and_then(|s| s.conditions.clone())
                                .unwrap_or_default();
                            let now = chrono::Utc::now().to_rfc3339();
                            if let Some(c) = conds.iter_mut().find(|c| {
                                matches!(c.type_, ConditionType::NfrObserved)
                            }) {
                                c.status = ConditionStatus::True;
                                c.reason = Some("PrometheusQueryOK".into());
                                c.message = Some("NFR signals observed".into());
                                c.last_transition_time = Some(now.clone());
                            } else {
                                conds.push(Condition {
                                    type_: ConditionType::NfrObserved,
                                    status: ConditionStatus::True,
                                    reason: Some("PrometheusQueryOK".into()),
                                    message: Some(
                                        "NFR signals observed".into(),
                                    ),
                                    last_transition_time: Some(now.clone()),
                                });
                            }
                            let dr_ns: Api<DeploymentRecord> =
                                Api::namespaced(ctx.client.clone(), &ns);
                            let status_patch = json!({
                                "status": {
                                    "nfr_recommendations": recs,
                                    "conditions": conds,
                                    "last_updated": now,
                                }
                            });
                            let _ = dr_ns
                                .patch_status(
                                    &name,
                                    &PatchParams::default(),
                                    &Patch::Merge(&status_patch),
                                )
                                .await;
                        }
                        Err(_) => { /* ignore and continue */ }
                    }
                }
            }
            Err(_) => { /* ignore listing errors */ }
        }
        tokio::time::sleep(Duration::from_secs(
            ctx.cfg.analyzer_interval_secs,
        ))
        .await;
    }
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
    // Render manifests via TemplateManager
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
            RenderedResource::Deployment(dep) => {
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
                // Apply dynamic resource (e.g., Knative Service)
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
        Some("service") => ScrapeKind::Service,
        Some("pod") => ScrapeKind::Pod,
        _ => ScrapeKind::Auto,
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

// owner_ref now provided and used inside TemplateManager

#[instrument(skip_all, fields(ns = %ns, name = %name, knative = %include_knative))]
async fn delete_children(
    client: &Client,
    ns: &str,
    name: &str,
    include_knative: bool,
) {
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp =
        kube::api::ListParams::default().labels(&owner_label_selector(name));

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

    // Best-effort delete Knative Service if CRD is present
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
                // Name equals class name for Knative Service
                let _ = dyn_api.delete(name, &DeleteParams::default()).await;
            }
        }
    }
}

fn into_internal<E: std::fmt::Display>(e: E) -> ReconcileErr {
    ReconcileErr::Internal(e.to_string())
}

fn error_policy(
    _obj: Arc<DeploymentRecord>,
    _error: &ReconcileErr,
    _ctx: Arc<ControllerContext>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_owner_label_selector_formats_correctly() {
        let s = owner_label_selector("my-class");
        assert_eq!(s, "oaas.io/owner=my-class");
    }

    #[test]
    fn test_compute_enable_odgm_true_only_when_flag_and_addon_present() {
        use crate::config::{CrmConfig, EnforcementConfig, FeaturesConfig};
        use crate::crd::deployment_record::DeploymentRecordSpec;

        let mut cfg = CrmConfig {
            profile: "dev".into(),
            http_port: 8088,
            grpc_port: 7088,
            k8s_namespace: "default".into(),
            security_mtls: None,
            features: FeaturesConfig {
                nfr_enforcement: None,
                hpa: None,
                knative: None,
                prometheus: None,
                leader_election: None,
                odgm_sidecar: Some(true),
            },
            enforcement: EnforcementConfig {
                cooldown_secs: 120,
                max_replica_delta_pct: 30,
                max_replicas: 20,
            },
            analyzer_interval_secs: 60,
        };
        // No addon -> false
        let spec = DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            function: None,
            nfr_requirements: None,
            nfr: None,
        };
        assert_eq!(compute_enable_odgm(&cfg, &spec), false);

        // With addon but feature disabled -> false
        cfg.features.odgm_sidecar = Some(false);
        let spec2 = DeploymentRecordSpec {
            selected_template: None,
            addons: Some(vec!["odgm".into()]),
            odgm_config: None,
            function: None,
            nfr_requirements: None,
            nfr: None,
        };
        assert_eq!(compute_enable_odgm(&cfg, &spec2), false);

        // Both enabled -> true
        cfg.features.odgm_sidecar = Some(true);
        assert_eq!(compute_enable_odgm(&cfg, &spec2), true);
    }

    #[test]
    fn test_build_progressing_status_sets_fields() {
        let st =
            build_progressing_status("2021-01-01T00:00:00Z".into(), Some(7));
        assert_eq!(st.phase.as_deref(), Some("Progressing"));
        assert_eq!(st.message.as_deref(), Some("Applied workload"));
        assert_eq!(st.observed_generation, Some(7));
        let conds = st.conditions.unwrap();
        assert!(conds.iter().any(|c| matches!(
            c.type_,
            ConditionType::Progressing
        ) && matches!(
            c.status,
            ConditionStatus::True
        )));
    }

    #[test]
    fn test_split_api_version_group_and_version() {
        let (g, v) = split_api_version("serving.knative.dev/v1");
        assert_eq!(g, "serving.knative.dev");
        assert_eq!(v, "v1");
        let (g2, v2) = split_api_version("v1");
        assert_eq!(g2, "v1");
        assert_eq!(v2, "");
    }

    #[test]
    fn test_dynamic_target_from_knative_manifest() {
        let manifest = serde_json::json!({
            "apiVersion": "serving.knative.dev/v1",
            "kind": "Service",
            "metadata": { "name": "class-z" }
        });
        let (gvk, name) = dynamic_target_from(
            "serving.knative.dev/v1",
            "Service",
            &manifest,
            "fallback",
        );
        assert_eq!(gvk.group, "serving.knative.dev");
        assert_eq!(gvk.version, "v1");
        assert_eq!(gvk.kind, "Service");
        assert_eq!(name, "class-z");
    }

    #[test]
    fn test_dynamic_target_from_uses_default_name_when_missing() {
        let manifest = serde_json::json!({
            "apiVersion": "group/v1",
            "kind": "Thing",
            "metadata": {}
        });
        let (_gvk, name) =
            dynamic_target_from("group/v1", "Thing", &manifest, "abc");
        assert_eq!(name, "abc");
    }
}
