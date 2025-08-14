use crate::config::{CrmConfig, PromConfig};
use envconfig::Envconfig;
use kube::{Client, api::Api, core::DynamicObject, discovery::ApiResource};
use serde_json::json;
use tracing::info;

#[derive(Clone, Debug)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub match_labels: Option<Vec<(String, String)>>,
    pub scrape_kind: ScrapeKind,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum ScrapeKind {
    Service,
    Pod,
    Auto,
}

impl Default for ScrapeKind {
    fn default() -> Self {
        ScrapeKind::Auto
    }
}

#[derive(Clone)]
pub struct PromOperatorProvider {
    client: Client,
}

impl PromOperatorProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn operator_crds_present(&self) -> bool {
        if let Ok(discovery) =
            kube::discovery::Discovery::new(self.client.clone())
                .run()
                .await
        {
            let has_group = discovery
                .groups()
                .any(|g| g.name() == "monitoring.coreos.com");
            return has_group;
        }
        false
    }

    pub async fn ensure_targets(
        &self,
        ns: &str,
        class_name: &str,
        app_label: &str,
        owner_label_key: &str,
        scrape_kind: ScrapeKind,
        match_labels: &Option<Vec<(String, String)>>,
        knative: bool,
    ) -> anyhow::Result<Vec<(String, String)>> {
        // When running on Knative and scrape_kind is Auto, delegate monitoring to Knative-level
        // configuration (per Knative docs) and do not create per-workload monitors.
        // If the user explicitly sets Service/Pod, we honor that override.
        if knative {
            match scrape_kind {
                ScrapeKind::Auto => {
                    tracing::info!(class = %class_name, "Knative enabled with scrape_kind=Auto: delegating metrics to Knative, skipping ServiceMonitor/PodMonitor creation");
                    return Ok(vec![]);
                }
                _ => { /* fall through and honor explicit mode */ }
            }
        }

        let scrape_kind = match (scrape_kind, knative) {
            // Knative + Auto handled above; default k8s path prefers Service-level scraping
            (ScrapeKind::Auto, _) => ScrapeKind::Service,
            (s, _) => s,
        };

        let mut sm_labels = json!({
            "oaas.io/owner": class_name,
            "app": app_label,
        });
        if let Some(list) = match_labels {
            for (k, v) in list.iter() {
                sm_labels[k] = json!(v);
            }
        }

        let mut refs: Vec<(String, String)> = Vec::new();
        match scrape_kind {
            ScrapeKind::Service => {
                let name = format!("{}-svcmon", class_name);
                let manifest = json!({
                        "apiVersion": "monitoring.coreos.com/v1",
                        "kind": "ServiceMonitor",
                        "metadata": {
                            "name": name,
                            "namespace": ns,
                            "labels": sm_labels,
                        },
                        "spec": {
                "selector": {"matchLabels": { owner_label_key: class_name, "app": app_label }},
                            "endpoints": [{ "port": "http", "path": "/metrics" }],
                            "namespaceSelector": {"matchNames": [ns]},
                        }
                    });
                self.apply_dynamic(ns, &manifest, "ServiceMonitor", &name)
                    .await?;
                refs.push(("ServiceMonitor".into(), name));
            }
            ScrapeKind::Pod => {
                let name = format!("{}-podmon", class_name);
                let manifest = json!({
                        "apiVersion": "monitoring.coreos.com/v1",
                        "kind": "PodMonitor",
                        "metadata": {
                            "name": name,
                            "namespace": ns,
                            "labels": sm_labels,
                        },
                        "spec": {
                "selector": {"matchLabels": { owner_label_key: class_name, "app": app_label }},
                            "podMetricsEndpoints": [{ "port": "http", "path": "/metrics" }],
                            "namespaceSelector": {"matchNames": [ns]},
                        }
                    });
                self.apply_dynamic(ns, &manifest, "PodMonitor", &name)
                    .await?;
                refs.push(("PodMonitor".into(), name));
            }
            ScrapeKind::Auto => {
                // Should not happen due to normalization above
                return Ok(refs);
            }
        }

        info!(class = %class_name, kind = ?scrape_kind, "ensured metrics targets");
        Ok(refs)
    }

    async fn apply_dynamic(
        &self,
        ns: &str,
        manifest: &serde_json::Value,
        kind: &str,
        name: &str,
    ) -> anyhow::Result<()> {
        let (group, version) = match kind {
            "ServiceMonitor" | "PodMonitor" => ("monitoring.coreos.com", "v1"),
            _ => anyhow::bail!("unsupported kind: {}", kind),
        };
        let gvk = kube::core::GroupVersionKind::gvk(group, version, kind);
        let ar = ApiResource::from_gvk(&gvk);
        let api: Api<DynamicObject> =
            Api::namespaced_with(self.client.clone(), ns, &ar);
        let pp = kube::api::PatchParams::apply("oprc-crm").force();
        api.patch(name, &pp, &kube::api::Patch::Apply(manifest))
            .await?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Analyzer {
    prom_base: Option<String>,
    knative_enabled: bool,
}

// Note: query_instant uses dynamic JSON parsing; typed structs removed to avoid dead_code warnings.

impl Analyzer {
    pub fn new() -> Self {
        let prom_base = PromConfig::init_from_env()
            .ok()
            .and_then(|c| c.url)
            .filter(|s| !s.is_empty());
        let knative_enabled = CrmConfig::init_from_env()
            .ok()
            .and_then(|c| c.features.knative)
            .unwrap_or(false);
        Self {
            prom_base,
            knative_enabled,
        }
    }

    pub fn with_config(
        prom_base: Option<String>,
        knative_enabled: bool,
    ) -> Self {
        Self {
            prom_base,
            knative_enabled,
        }
    }

    #[tracing::instrument(skip(self), fields(ns=%ns, name=%name, knative=%self.knative_enabled))]
    pub async fn observe_only(
        &self,
        ns: &str,
        name: &str,
    ) -> anyhow::Result<Vec<crate::crd::deployment_record::NfrRecommendation>>
    {
        // Use cached Prometheus base and Knative flag captured at construction time
        let base = match &self.prom_base {
            Some(v) => v.clone(),
            None => return Ok(vec![]),
        };
        let client = reqwest::Client::new();
        // Simple instant queries as placeholders; windowed queries can be added later
        let lbl = format!("oaas_owner=\"{}\",namespace=\"{}\"", name, ns);
        // For Knative, use activator_request_count; otherwise, skip RPS generation
        let rps_q_kn = format!(
            "sum(rate(activator_request_count{{namespace_name=\"{}\", configuration_name=\"{}\"}}[1m]))",
            ns, name
        );
        // Latency p99: use Knative activator histogram when in Knative, else fallback to http server histogram
        let p99_q = if self.knative_enabled {
            // Follow Knative dashboard style: compute per-revision quantiles then take the max across revisions
            format!(
                "1000 * max(histogram_quantile(0.99, sum(rate(activator_request_latencies_bucket{{namespace_name=\"{}\", configuration_name=\"{}\"}}[1m])) by (revision_name, le)))",
                ns, name
            )
        } else {
            format!(
                "1000 * histogram_quantile(0.99, sum(rate(http_server_requests_seconds_bucket{{{}}}[5m])) by (le))",
                lbl
            )
        };

        let rps = if self.knative_enabled {
            self.query_instant(&client, &base, &rps_q_kn)
                .await
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let p99_ms = self
            .query_instant(&client, &base, &p99_q)
            .await
            .unwrap_or(0.0);

        // CPU and Memory using cAdvisor metrics with pod prefix fallback
        let pod_prefix = format!("{}-", name);
        let cpu_q = format!(
            "1000 * sum(rate(container_cpu_usage_seconds_total{{namespace=\"{}\", pod=~\"{}.*\", container!=\"\"}}[5m]))",
            ns, pod_prefix
        );
        let mem_q = format!(
            "sum(container_memory_working_set_bytes{{namespace=\"{}\", pod=~\"{}.*\", container!=\"\"}})",
            ns, pod_prefix
        );
        let cpu_mcores = self
            .query_instant(&client, &base, &cpu_q)
            .await
            .unwrap_or(0.0);
        let mem_bytes = self
            .query_instant(&client, &base, &mem_q)
            .await
            .unwrap_or(0.0);

        let mut recs = vec![];
        if rps > 0.0 {
            recs.push(crate::crd::deployment_record::NfrRecommendation {
                component: "function".into(),
                dimension: "replicas".into(),
                target: self.heuristic_replicas(rps, p99_ms),
                basis: Some("rps/p99 over 5m".into()),
                confidence: Some(0.5),
            });
        }
        if cpu_mcores > 0.0 {
            recs.push(crate::crd::deployment_record::NfrRecommendation {
                component: "function".into(),
                dimension: "cpu".into(),
                target: (cpu_mcores * 1.3).ceil(), // headroom
                basis: Some("sum of container cpu mcores over 5m".into()),
                confidence: Some(0.6),
            });
        }
        if mem_bytes > 0.0 {
            recs.push(crate::crd::deployment_record::NfrRecommendation {
                component: "function".into(),
                dimension: "memory".into(),
                target: (mem_bytes * 1.2).ceil(), // headroom
                basis: Some("sum of working set bytes".into()),
                confidence: Some(0.6),
            });
        }
        Ok(recs)
    }

    #[tracing::instrument(skip(self, client), fields(expr=%expr))]
    async fn query_instant(
        &self,
        client: &reqwest::Client,
        base: &str,
        expr: &str,
    ) -> anyhow::Result<f64> {
        let url = format!("{}/api/v1/query", base.trim_end_matches('/'));
        let res = client.get(url).query(&[("query", expr)]).send().await?;
        if !res.status().is_success() {
            return Ok(0.0);
        }
        let body: serde_json::Value = res.json().await?;
        let v = body
            .get("data")
            .and_then(|d| d.get("result"))
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first())
            .and_then(|s| s.get("value"))
            .and_then(|val| val.as_array())
            .and_then(|arr| arr.get(1))
            .and_then(|s| s.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(v)
    }

    fn heuristic_replicas(&self, rps: f64, p99_ms: f64) -> f64 {
        // naive: base replicas on rps and tighten slightly if latency high
        let base = (rps / 50.0).ceil().max(1.0);
        if p99_ms > 250.0 { base + 1.0 } else { base }
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod analyzer_tests {
    use super::Analyzer;

    #[test]
    fn replicas_heuristic_increases_with_rps_and_latency() {
        let a = Analyzer::new();
        assert_eq!(a.heuristic_replicas(0.0, 0.0), 1.0);
        assert_eq!(a.heuristic_replicas(49.0, 0.0), 1.0);
        assert_eq!(a.heuristic_replicas(50.0, 0.0), 1.0);
        assert_eq!(a.heuristic_replicas(51.0, 0.0), 2.0);
        assert_eq!(a.heuristic_replicas(100.0, 300.0), 3.0); // +1 for high latency
    }
}

pub fn parse_match_labels(s: &str) -> Vec<(String, String)> {
    s.split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_match_labels_splits_and_trims() {
        let v = parse_match_labels("release=prom, team = core ");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], ("release".into(), "prom".into()));
        assert_eq!(v[1], ("team".into(), "core".into()));
    }
}
