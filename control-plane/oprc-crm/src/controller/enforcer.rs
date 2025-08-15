use std::collections::HashMap;
use std::sync::Arc;

use kube::ResourceExt;
use kube::api::{Api, Patch, PatchParams};
use serde_json::json;
use tokio::time::Duration;
use tracing::{debug, trace, warn};

use crate::crd::deployment_record::DeploymentRecord;

use super::{ControllerContext, build_obj_ref};

#[derive(Debug, PartialEq, Eq)]
pub enum EnforceDecision {
    SkipCooldown,
    SkipStability,
    Apply,
}

#[derive(Default, Clone)]
pub struct StabilityState {
    pub last_target: Option<u32>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub cooldown_until: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn eval_enforce(
    state: &mut StabilityState,
    target: u32,
    now: chrono::DateTime<chrono::Utc>,
    stability_secs: u64,
    cooldown_secs: u64,
) -> EnforceDecision {
    if let Some(until) = state.cooldown_until {
        if now < until {
            return EnforceDecision::SkipCooldown;
        }
    }
    if state.last_target == Some(target) {
        if state.since.is_none() {
            state.since = Some(now);
            return EnforceDecision::SkipStability;
        }
        let since = state.since.unwrap();
        if (now - since).num_seconds() as u64 >= stability_secs {
            state.since = None;
            state.cooldown_until =
                Some(now + chrono::Duration::seconds(cooldown_secs as i64));
            return EnforceDecision::Apply;
        } else {
            return EnforceDecision::SkipStability;
        }
    } else {
        state.last_target = Some(target);
        state.since = Some(now);
        EnforceDecision::SkipStability
    }
}

pub async fn enforcer_loop(ctx: Arc<ControllerContext>) {
    let mut state: HashMap<String, StabilityState> = HashMap::new();
    loop {
        let items: Vec<DeploymentRecord> = {
            let r = ctx.dr_cache.read().await;
            r.values().cloned().collect()
        };
        debug!(
            cache_size = items.len(),
            interval_secs = ctx.cfg.analyzer_interval_secs,
            "enforcer: tick"
        );
        for dr in items {
            if dr.metadata.deletion_timestamp.is_some() {
                continue;
            }
            let ns = match dr.namespace() {
                Some(n) => n,
                None => continue,
            };
            let name = dr.name_any();
            let mode = dr
                .spec
                .nfr
                .as_ref()
                .and_then(|n| n.enforcement.as_ref())
                .and_then(|e| e.mode.clone())
                .unwrap_or_else(|| "observe".into());
            if mode != "enforce" {
                trace!(%ns, %name, %mode, "enforcer: skip (mode)");
                continue;
            }
            let dims = dr
                .spec
                .nfr
                .as_ref()
                .and_then(|n| n.enforcement.as_ref())
                .and_then(|e| e.dimensions.clone())
                .unwrap_or_default();
            if !dims.iter().any(|d| d == "replicas") {
                trace!(%ns, %name, "enforcer: skip (no replicas dimension)");
                continue;
            }

            let recs = dr
                .status
                .as_ref()
                .and_then(|s| s.nfr_recommendations.clone())
                .unwrap_or_default();
            let replicas_rec = recs
                .iter()
                .find(|r| {
                    r.dimension == "replicas" && r.component == "function"
                })
                .map(|r| {
                    r.target
                        .max(1.0)
                        .min(ctx.cfg.enforcement.max_replicas as f64)
                        as u32
                });
            if replicas_rec.is_none() {
                trace!(%ns, %name, "enforcer: skip (no replicas recommendation)");
                continue;
            }
            let target = replicas_rec.unwrap();

            let key = format!("{}/{}", ns, name);
            let entry = state.entry(key.clone()).or_default();
            let now = chrono::Utc::now();
            match eval_enforce(
                entry,
                target,
                now,
                ctx.cfg.enforcement.stability_secs,
                ctx.cfg.enforcement.cooldown_secs,
            ) {
                EnforceDecision::SkipCooldown => {
                    trace!(%ns, %name, "enforcer: cooldown active");
                    continue;
                }
                EnforceDecision::SkipStability => {
                    trace!(%ns, %name, target, "enforcer: waiting for stability");
                    continue;
                }
                EnforceDecision::Apply => {
                    debug!(%ns, %name, target, "enforcer: applying replicas min");
                    if let Err(e) =
                        apply_replicas_min_hpa_aware(&ctx, &ns, &name, target)
                            .await
                    {
                        warn!(%ns, %name, error=?e, "enforcer: apply failed");
                        continue;
                    }
                    let dr_api: Api<DeploymentRecord> =
                        Api::namespaced(ctx.client.clone(), &ns);
                    let now_s = chrono::Utc::now().to_rfc3339();
                    let applied = json!({
                        "status": {
                            "last_applied_recommendations": [{
                                "component": "function",
                                "dimension": "replicas",
                                "target": target as f64,
                                "basis": "enforcer" ,
                                "confidence": 1.0
                            }],
                            "last_applied_at": now_s,
                        }
                    });
                    let _ = dr_api
                        .patch_status(
                            &name,
                            &PatchParams::default(),
                            &Patch::Merge(&applied),
                        )
                        .await;

                    // Emit NFRApplied event to signal successful enforcement
                    let _ = ctx
                        .event_recorder
                        .publish(
                            &kube::runtime::events::Event {
                                type_: kube::runtime::events::EventType::Normal,
                                reason: "NFRApplied".into(),
                                note: Some(format!(
                                    "Applied NFR enforcement: replicas -> {}",
                                    target
                                )),
                                action: "Enforce".into(),
                                secondary: None,
                            },
                            &build_obj_ref(&ns, &name, None),
                        )
                        .await;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(ctx.cfg.analyzer_interval_secs))
            .await;
    }
}

#[allow(unused_variables)]
async fn apply_replicas_min(
    ctx: &Arc<ControllerContext>,
    ns: &str,
    name: &str,
    min: u32,
) -> anyhow::Result<()> {
    if ctx.include_knative {
        use kube::core::DynamicObject;
        use kube::core::GroupVersionKind;
        use kube::discovery::ApiResource;
        let gvk = GroupVersionKind::gvk("serving.knative.dev", "v1", "Service");
        let (api, res_name) = (
            kube::Api::<DynamicObject>::namespaced_with(
                ctx.client.clone(),
                ns,
                &ApiResource::from_gvk(&gvk),
            ),
            name.to_string(),
        );
        let patch = json!({
            "spec": {"template": {"metadata": {"annotations": {
                "autoscaling.knative.dev/minScale": min.to_string()
            }}}}
        });
        debug!(%ns, %name, min, path = "knative", "apply replicas min");
        let _ = api
            .patch(
                &res_name,
                &PatchParams::apply("oprc-crm").force(),
                &kube::api::Patch::<serde_json::Value>::Apply(patch),
            )
            .await?;
        return Ok(());
    }
    let dapi: kube::Api<k8s_openapi::api::apps::v1::Deployment> =
        kube::Api::namespaced(ctx.client.clone(), ns);
    let patch = json!({"spec": {"replicas": min }});
    debug!(%ns, %name, min, path = "k8s", "apply replicas min");
    let _ = dapi
        .patch(name, &PatchParams::default(), &Patch::Merge(&patch))
        .await?;
    Ok(())
}

async fn apply_replicas_min_hpa_aware(
    ctx: &Arc<ControllerContext>,
    ns: &str,
    name: &str,
    min: u32,
) -> anyhow::Result<()> {
    let hpa_enabled = ctx.cfg.features.hpa.unwrap_or(false);
    if hpa_enabled && !ctx.include_knative {
        let api: kube::Api<
            k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler,
        > = kube::Api::namespaced(ctx.client.clone(), ns);
        let hpa_name = name;
        let patch = json!({
            "spec": { "minReplicas": min }
        });
        debug!(%ns, %name, min, path = "hpa", "apply replicas min");
        match api
            .patch(hpa_name, &PatchParams::default(), &Patch::Merge(&patch))
            .await
        {
            Ok(_) => return Ok(()),
            Err(_e) => {
                trace!(%ns, %name, "hpa patch failed; falling back to deployment/knative");
            }
        }
    }
    debug!(%ns, %name, min, "falling back to non-HPA replicas enforcement");
    apply_replicas_min(ctx, ns, name, min).await
}
