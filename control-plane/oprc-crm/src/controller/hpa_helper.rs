use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use kube::api::{Api, Patch, PatchParams};
use serde_json::json;
use tracing::{debug, trace, warn};

use std::time::Duration;

pub async fn patch_hpa_minreplicas(
    api: &Api<HorizontalPodAutoscaler>,
    name: &str,
    min: u32,
    max_retries: usize,
) -> anyhow::Result<()> {
    let patch = json!({"spec": {"minReplicas": min}});
    let params = PatchParams::default();
    let mut attempt = 0;
    loop {
        match api.patch(name, &params, &Patch::Merge(&patch)).await {
            Ok(_) => {
                debug!(%name, min, "hpa patched");
                return Ok(());
            }
            Err(e) => {
                attempt += 1;
                // NotFound or Forbidden: don't retry
                let msg = e.to_string();
                if msg.contains("NotFound") || msg.contains("forbidden") {
                    trace!(%name, error=%msg, "hpa patch non-retryable");
                    return Err(anyhow::anyhow!(e));
                }
                if attempt >= max_retries {
                    warn!(%name, error=%msg, "hpa patch retries exhausted");
                    return Err(anyhow::anyhow!(e));
                }
                let backoff =
                    Duration::from_millis(100 * (1 << (attempt - 1)) as u64);
                trace!(%name, attempt, backoff_ms = backoff.as_millis() as u64, "hpa patch retry");
                tokio::time::sleep(backoff).await;
            }
        }
    }
}
