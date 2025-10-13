use std::collections::HashMap;
use std::sync::Arc;

use kube::ResourceExt;
use kube::api::{Api, Patch, PatchParams};
use serde_json::json;
use tokio::time::Duration;
use tracing::{debug, instrument, trace, warn};

use crate::crd::class_runtime::ClassRuntime;

use super::ControllerContext;
use crate::controller::events::{REASON_NFR_APPLIED, emit_event};
use crate::controller::hpa_helper::patch_hpa_minreplicas;
use crate::controller::types::EnforcementMode;

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

#[instrument(level="debug", skip(ctx), fields(interval_secs = ctx.cfg.analyzer_interval_secs))]
pub async fn enforcer_loop(ctx: Arc<ControllerContext>) {
    let mut state: HashMap<String, StabilityState> = HashMap::new();
    loop {
        let items: Vec<ClassRuntime> = ctx.dr_cache.list().await;
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
            let mode_s = dr
                .spec
                .enforcement
                .as_ref()
                .and_then(|e| e.mode.clone())
                .unwrap_or_else(|| "observe".into());
            let mode = EnforcementMode::from_str(&mode_s);
            if mode != EnforcementMode::Enforce {
                trace!(%ns, %name, %mode, "enforcer: skip (mode)");
                continue;
            }
            let dims = dr
                .spec
                .enforcement
                .as_ref()
                .and_then(|e| e.dimensions.clone())
                .unwrap_or_default();
            if !dims.iter().any(|d| d == "replicas") {
                trace!(%ns, %name, "enforcer: skip (no replicas dimension)");
                continue;
            }

            // nfr_recommendations is stored as a JSON object in the status (chart CRD)
            // Try to obtain a replicas recommendation from the cached status first.
            // If the cached DR doesn't contain recommendations (common when status was
            // patched directly and the subsequent reconcile event didn't carry status),
            // fall back to fetching the live CR from the API to read the latest status.
            let mut replicas_rec = dr
                .status
                .as_ref()
                .and_then(|s| s.nfr_recommendations.clone())
                .and_then(|map| map.get("replicas").cloned())
                .and_then(|val| match val {
                    serde_json::Value::Number(n) => {
                        n.as_u64().map(|u| u as u32)
                    }
                    serde_json::Value::String(s) => s.parse::<u32>().ok(),
                    _ => None,
                })
                .map(|r| r.max(1).min(ctx.cfg.enforcement.max_replicas));

            if replicas_rec.is_none() {
                // Try to read the live CR from the API as a fallback to catch status-only patches
                // that may not be reflected in the controller's cached object.
                let dr_api: Api<ClassRuntime> =
                    Api::namespaced(ctx.client.clone(), &ns);
                if let Ok(opt_live) = dr_api.get_opt(&name).await {
                    if let Some(live) = opt_live {
                        replicas_rec = live
                            .status
                            .and_then(|s| s.nfr_recommendations.clone())
                            .and_then(|map| map.get("replicas").cloned())
                            .and_then(|val| match val {
                                serde_json::Value::Number(n) => {
                                    n.as_u64().map(|u| u as u32)
                                }
                                serde_json::Value::String(s) => {
                                    s.parse::<u32>().ok()
                                }
                                _ => None,
                            })
                            .map(|r| {
                                r.max(1).min(ctx.cfg.enforcement.max_replicas)
                            });
                    }
                }
            }
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
                    let dr_api: Api<ClassRuntime> =
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
                    let res = dr_api
                        .patch_status(
                            &name,
                            &PatchParams::default(),
                            &Patch::Merge(&applied),
                        )
                        .await;

                    // Update local cache with applied recommendations to keep shapes consistent
                    if res.is_ok() {
                        // attempt to update cache entry
                        if let Some(current) =
                            ctx.dr_cache.list().await.into_iter().find(|d| {
                                d.name_any() == name
                                    && d.namespace().as_deref()
                                        == Some(ns.as_str())
                            })
                        {
                            let mut updated = current.clone();
                            let now_s2 = chrono::Utc::now().to_rfc3339();
                            if let Some(ref mut s) = updated.status {
                                s.last_applied_recommendations = Some(vec![crate::crd::class_runtime::NfrRecommendation {
                                    component: "function".into(),
                                    dimension: "replicas".into(),
                                    target: target as f64,
                                    basis: Some("enforcer".into()),
                                    confidence: Some(1.0),
                                }]);
                                s.last_applied_at = Some(now_s2);
                            } else {
                                updated.status = Some(crate::crd::class_runtime::ClassRuntimeStatus {
                                    last_applied_recommendations: Some(vec![crate::crd::class_runtime::NfrRecommendation {
                                        component: "function".into(),
                                        dimension: "replicas".into(),
                                        target: target as f64,
                                        basis: Some("enforcer".into()),
                                        confidence: Some(1.0),
                                    }]),
                                    last_applied_at: Some(now_s2),
                                    routers: None,
                                    ..Default::default()
                                });
                            }
                            ctx.dr_cache
                                .upsert(format!("{}/{}", ns, name), updated)
                                .await;
                        }
                    }

                    // Emit NFRApplied event to signal successful enforcement
                    emit_event(
                        &ctx.event_recorder,
                        &ns,
                        &name,
                        None,
                        REASON_NFR_APPLIED,
                        "Enforce",
                        Some(format!(
                            "Applied NFR enforcement: replicas -> {}",
                            target
                        )),
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
#[instrument(level="debug", skip(ctx), fields(ns=%ns, name=%name, min))]
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
    match dapi
        .patch(name, &PatchParams::default(), &Patch::Merge(&patch))
        .await
    {
        Ok(_) => {
            debug!(%ns, %name, min, "deployment patched successfully");
        }
        Err(e) => {
            warn!(%ns, %name, error = ?e, "deployment patch failed");
            return Err(e.into());
        }
    }
    Ok(())
}

#[instrument(level="debug", skip(ctx), fields(ns=%ns, name=%name, min))]
async fn apply_replicas_min_hpa_aware(
    ctx: &Arc<ControllerContext>,
    ns: &str,
    name: &str,
    min: u32,
) -> anyhow::Result<()> {
    let hpa_enabled = ctx.cfg.features.hpa.unwrap_or(false);
    if hpa_enabled && !ctx.include_knative {
        use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
        let api: kube::Api<HorizontalPodAutoscaler> =
            kube::Api::namespaced(ctx.client.clone(), ns);
        debug!(%ns, %name, min, path = "hpa", "apply replicas min");
        if patch_hpa_minreplicas(&api, name, min, 3).await.is_ok() {
            return Ok(());
        } else {
            trace!(%ns, %name, "hpa patch failed; falling back to deployment/knative");
        }
    }
    debug!(%ns, %name, min, "falling back to non-HPA replicas enforcement");
    apply_replicas_min(ctx, ns, name, min).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_enforce_waits_then_applies_then_cooldowns() {
        let mut state = StabilityState::default();
        let t0 = chrono::Utc::now();
        let stability = 2u64;
        let cooldown = 3u64;

        // First observation sets target and starts stability window
        let d1 = eval_enforce(&mut state, 3, t0, stability, cooldown);
        assert_eq!(d1, EnforceDecision::SkipStability);

        // Before stability elapsed, still SkipStability
        let d2 = eval_enforce(
            &mut state,
            3,
            t0 + chrono::Duration::seconds(1),
            stability,
            cooldown,
        );
        assert_eq!(d2, EnforceDecision::SkipStability);

        // After stability period, we should Apply
        let d3 = eval_enforce(
            &mut state,
            3,
            t0 + chrono::Duration::seconds(2),
            stability,
            cooldown,
        );
        assert_eq!(d3, EnforceDecision::Apply);

        // Immediately after apply, cooldown should be active
        let d4 = eval_enforce(
            &mut state,
            3,
            t0 + chrono::Duration::seconds(2),
            stability,
            cooldown,
        );
        assert_eq!(d4, EnforceDecision::SkipCooldown);

        // After cooldown expires, stability should start again and skip first
        let d5 = eval_enforce(
            &mut state,
            3,
            t0 + chrono::Duration::seconds(2 + 3),
            stability,
            cooldown,
        );
        assert_eq!(d5, EnforceDecision::SkipStability);
    }

    #[test]
    fn eval_enforce_resets_on_target_change() {
        let mut state = StabilityState::default();
        let t0 = chrono::Utc::now();
        let stability = 5u64;
        let cooldown = 5u64;

        // Start with target 2
        let d1 = eval_enforce(&mut state, 2, t0, stability, cooldown);
        assert_eq!(d1, EnforceDecision::SkipStability);
        assert_eq!(state.last_target, Some(2));

        // Change to target 5 -> resets stability window
        let d2 = eval_enforce(
            &mut state,
            5,
            t0 + chrono::Duration::seconds(1),
            stability,
            cooldown,
        );
        assert_eq!(d2, EnforceDecision::SkipStability);
        assert_eq!(state.last_target, Some(5));

        // With insufficient time at target 5, still SkipStability
        let d3 = eval_enforce(
            &mut state,
            5,
            t0 + chrono::Duration::seconds(3),
            stability,
            cooldown,
        );
        assert_eq!(d3, EnforceDecision::SkipStability);
    }
}
