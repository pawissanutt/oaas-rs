use std::sync::Arc;

use kube::ResourceExt;
use kube::api::{Api, Patch, PatchParams};
use serde_json::json;
use tokio::time::Duration;
use tracing::{debug, trace};

use crate::crd::deployment_record::{
    Condition, ConditionStatus, ConditionType, DeploymentRecord,
};

use super::ControllerContext;

pub async fn analyzer_loop(ctx: Arc<ControllerContext>) {
    loop {
        if !ctx.metrics_enabled {
            tokio::time::sleep(Duration::from_secs(
                ctx.cfg.analyzer_interval_secs,
            ))
            .await;
            continue;
        }
        // Snapshot DRs from local cache to avoid holding the lock across awaits
        let items: Vec<DeploymentRecord> = ctx.dr_cache.list().await;
        debug!(
            cache_size = items.len(),
            interval_secs = ctx.cfg.analyzer_interval_secs,
            "analyzer: tick"
        );
        {
            for dr in items {
                if dr.metadata.deletion_timestamp.is_some() {
                    continue;
                }
                let ns: String = match dr.namespace() {
                    Some(n) => n,
                    None => continue,
                };
                let name = dr.name_any();
                let target_rps = dr
                    .spec
                    .nfr_requirements
                    .as_ref()
                    .and_then(|n| n.min_throughput_rps)
                    .map(|v| v as f64);
                match ctx
                    .analyzer
                    .observe_only(&ns, &name, target_rps, None)
                    .await
                {
                    Ok(recs) => {
                        let replicas_target = recs
                            .iter()
                            .find(|r| {
                                r.dimension == "replicas"
                                    && r.component == "function"
                            })
                            .map(|r| r.target);
                        debug!(%ns, %name, recs_len = recs.len(), replicas_target = ?replicas_target, "analyzer: recommendations computed");
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
                                message: Some("NFR signals observed".into()),
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
                        if dr_ns
                            .patch_status(
                                &name,
                                &PatchParams::default(),
                                &Patch::Merge(&status_patch),
                            )
                            .await
                            .is_ok()
                        {
                            // Update local cache with new status snapshot using typed values
                            let mut updated = dr.clone();
                            if let Some(ref mut s) = updated.status {
                                s.nfr_recommendations = Some(recs.clone());
                                s.conditions = Some(conds.clone());
                                s.last_updated = Some(now.clone());
                            } else {
                                updated.status = Some(crate::crd::deployment_record::DeploymentRecordStatus {
                                        phase: None,
                                        message: None,
                                        observed_generation: dr
                                            .status
                                            .as_ref()
                                            .and_then(|st| st.observed_generation),
                                        last_updated: Some(now.clone()),
                                        conditions: Some(conds.clone()),
                                        resource_refs: dr
                                            .status
                                            .as_ref()
                                            .and_then(|st| st.resource_refs.clone()),
                                        nfr_recommendations: Some(recs.clone()),
                                        last_applied_recommendations: None,
                                        last_applied_at: None,
                                    });
                            }
                            ctx.dr_cache
                                .upsert(format!("{}/{}", ns, name), updated)
                                .await;
                            trace!(%ns, %name, "analyzer: patched status + cache updated");
                        }
                    }
                    Err(e) => {
                        trace!(%ns, %name, error=?e, "analyzer: observe_only error");
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(ctx.cfg.analyzer_interval_secs))
            .await;
    }
}
