use crate::config::{CrmConfig, PromConfig};
use envconfig::Envconfig;
use tracing::debug;

#[derive(Clone, Debug)]
pub struct Analyzer {
    prom_base: Option<String>,
    knative_enabled: bool,
    // Range query settings (seconds)
    prom_range_secs: u64,
    prom_step_secs: u64,
    query_timeout_secs: u64,
}

// Note: query_instant uses dynamic JSON parsing; typed structs removed to avoid dead_code warnings.

impl Analyzer {
    pub fn new() -> Self {
        let pc = PromConfig::init_from_env().ok().unwrap_or_default();
        let prom_base = pc.url.filter(|s| !s.is_empty());
        let knative_enabled = CrmConfig::init_from_env()
            .ok()
            .map(|c| c.features.knative)
            .unwrap_or(false);
        let prom_range_secs = parse_duration_secs(&pc.range).unwrap_or(600);
        let prom_step_secs = parse_duration_secs(&pc.step).unwrap_or(30);
        let query_timeout_secs = pc.query_timeout_secs;
        Self {
            prom_base,
            knative_enabled,
            prom_range_secs,
            prom_step_secs,
            query_timeout_secs,
        }
    }

    pub fn with_config(
        prom_base: Option<String>,
        knative_enabled: bool,
    ) -> Self {
        // Keep defaults for range/step when using with_config in tests
        let pc = PromConfig::init_from_env().ok().unwrap_or_default();
        Self {
            prom_base,
            knative_enabled,
            prom_range_secs: parse_duration_secs(&pc.range).unwrap_or(600),
            prom_step_secs: parse_duration_secs(&pc.step).unwrap_or(30),
            query_timeout_secs: pc.query_timeout_secs,
        }
    }

    /// Returns true if Prometheus URL is configured
    pub fn is_configured(&self) -> bool {
        self.prom_base.is_some()
    }

    #[tracing::instrument(skip(self), fields(ns=%ns, name=%name, knative=%self.knative_enabled))]
    pub async fn observe_only(
        &self,
        ns: &str,
        name: &str,
        target_rps: Option<f64>,
        req_cpu_per_pod_m: Option<f64>,
    ) -> anyhow::Result<Vec<crate::crd::class_runtime::NfrRecommendation>> {
        // Use cached Prometheus base and Knative flag captured at construction time
        let base = match &self.prom_base {
            Some(v) => v.clone(),
            None => return Ok(vec![]),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.query_timeout_secs))
            .build()?;
        // Prefer windowed range queries with aggregation; instant queries serve as fallback
        let lbl = format!("oaas_owner=\"{}\",namespace=\"{}\"", name, ns);
        // For Knative, use activator_request_count; non‑Knative RPS is disabled for now
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

        let (start_ts, end_ts) = time_range_now(self.prom_range_secs);
        let rps = if self.knative_enabled {
            match self
                .query_range_aggregate(
                    &client,
                    &base,
                    &rps_q_kn,
                    start_ts,
                    end_ts,
                    self.prom_step_secs,
                    Aggregate::Avg,
                )
                .await
            {
                Ok(v) if v > 0.0 => {
                    debug!(metric="rps", src="range", value=%v, range_secs=%self.prom_range_secs, step_secs=%self.prom_step_secs);
                    v
                }
                _ => {
                    let v = self
                        .query_instant(&client, &base, &rps_q_kn)
                        .await
                        .unwrap_or(0.0);
                    debug!(metric="rps", src="instant", value=%v);
                    v
                }
            }
        } else {
            0.0
        };
        let p99_ms = match self
            .query_range_aggregate(
                &client,
                &base,
                &p99_q,
                start_ts,
                end_ts,
                self.prom_step_secs,
                Aggregate::Avg,
            )
            .await
        {
            Ok(v) if v > 0.0 => {
                debug!(metric="p99_ms", src="range", value=%v, range_secs=%self.prom_range_secs, step_secs=%self.prom_step_secs);
                v
            }
            _ => {
                let v = self
                    .query_instant(&client, &base, &p99_q)
                    .await
                    .unwrap_or(0.0);
                debug!(metric="p99_ms", src="instant", value=%v);
                v
            }
        };

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
        let cpu_mcores = match self
            .query_range_aggregate(
                &client,
                &base,
                &cpu_q,
                start_ts,
                end_ts,
                self.prom_step_secs,
                Aggregate::Avg,
            )
            .await
        {
            Ok(v) if v > 0.0 => {
                debug!(metric="cpu_mcores", src="range", value=%v, range_secs=%self.prom_range_secs, step_secs=%self.prom_step_secs);
                v
            }
            _ => {
                let v = self
                    .query_instant(&client, &base, &cpu_q)
                    .await
                    .unwrap_or(0.0);
                debug!(metric="cpu_mcores", src="instant", value=%v);
                v
            }
        };
        let mem_bytes = match self
            .query_range_aggregate(
                &client,
                &base,
                &mem_q,
                start_ts,
                end_ts,
                self.prom_step_secs,
                Aggregate::Avg,
            )
            .await
        {
            Ok(v) if v > 0.0 => {
                debug!(metric="mem_bytes", src="range", value=%v, range_secs=%self.prom_range_secs, step_secs=%self.prom_step_secs);
                v
            }
            _ => {
                let v = self
                    .query_instant(&client, &base, &mem_q)
                    .await
                    .unwrap_or(0.0);
                debug!(metric="mem_bytes", src="instant", value=%v);
                v
            }
        };

        let mut recs = vec![];
        if cpu_mcores > 0.0 {
            let req_cpu_per_pod_m = req_cpu_per_pod_m.unwrap_or(500.0);
            let replicas = if let Some(tgt_rps) = target_rps {
                // replica = target_rps / current_rps * total_current_cpu / req_cpu_per_pod
                let ratio = if rps > 0.0 { tgt_rps / rps } else { 1.0 };
                let v =
                    (ratio * (cpu_mcores / req_cpu_per_pod_m)).ceil().max(1.0);
                let v = if p99_ms > 250.0 { v + 1.0 } else { v };
                debug!(
                    metric="replicas_calc_rps_cpu",
                    target_rps=?target_rps,
                    current_rps=%rps,
                    total_cpu_mcores=%cpu_mcores,
                    req_cpu_per_pod_m=%req_cpu_per_pod_m,
                    p99_ms=%p99_ms,
                    replicas=%v,
                    "replicas from target/current rps and cpu"
                );
                v
            } else {
                self.heuristic_replicas_cpu(cpu_mcores, p99_ms)
            };
            recs.push(crate::crd::class_runtime::NfrRecommendation {
                component: "function".into(),
                dimension: "replicas".into(),
                target: replicas,
                basis: Some(format!(
                    "replicas = target_rps/current_rps * total_cpu/{}m over {}s",
                    req_cpu_per_pod_m,
                    self.prom_range_secs
                )),
                confidence: Some(0.6),
            });
        }
        if cpu_mcores > 0.0 {
            recs.push(crate::crd::class_runtime::NfrRecommendation {
                component: "function".into(),
                dimension: "cpu".into(),
                target: (cpu_mcores * 1.3).ceil(), // headroom
                basis: Some(format!(
                    "avg of container cpu mcores over {}s",
                    self.prom_range_secs
                )),
                confidence: Some(0.6),
            });
        }
        if mem_bytes > 0.0 {
            recs.push(crate::crd::class_runtime::NfrRecommendation {
                component: "function".into(),
                dimension: "memory".into(),
                target: (mem_bytes * 1.2).ceil(), // headroom
                basis: Some(format!(
                    "avg of working set bytes over {}s",
                    self.prom_range_secs
                )),
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

    #[tracing::instrument(skip(self, client), fields(expr=%expr, start=%start_ts, end=%end_ts, step=%step_secs))]
    async fn query_range_aggregate(
        &self,
        client: &reqwest::Client,
        base: &str,
        expr: &str,
        start_ts: i64,
        end_ts: i64,
        step_secs: u64,
        mode: Aggregate,
    ) -> anyhow::Result<f64> {
        if end_ts <= start_ts {
            return Ok(0.0);
        }
        let url = format!("{}/api/v1/query_range", base.trim_end_matches('/'));
        let res = client
            .get(url)
            .query(&[
                ("query", expr),
                ("start", &start_ts.to_string()),
                ("end", &end_ts.to_string()),
                ("step", &step_secs.to_string()),
            ])
            .send()
            .await?;
        if !res.status().is_success() {
            return Ok(0.0);
        }
        let body: serde_json::Value = res.json().await?;
        let mut values: Vec<f64> = Vec::new();
        if let Some(results) = body
            .get("data")
            .and_then(|d| d.get("result"))
            .and_then(|r| r.as_array())
        {
            for series in results {
                if let Some(vs) =
                    series.get("values").and_then(|v| v.as_array())
                {
                    for pair in vs {
                        if let Some(val) = pair
                            .as_array()
                            .and_then(|arr| arr.get(1))
                            .and_then(|s| s.as_str())
                            .and_then(|s| s.parse::<f64>().ok())
                        {
                            values.push(val);
                        }
                    }
                }
            }
        }
        if values.is_empty() {
            return Ok(0.0);
        }
        let out = match mode {
            Aggregate::Avg => {
                values.iter().copied().sum::<f64>() / (values.len() as f64)
            }
            Aggregate::Max => values
                .into_iter()
                .fold(f64::MIN, |a, b| if b > a { b } else { a }),
        };
        Ok(out)
    }

    fn heuristic_replicas_cpu(&self, cpu_mcores: f64, p99_ms: f64) -> f64 {
        // Simple CPU-based heuristic: target ~500m per replica, add one if latency is high
        let per_replica_target_m = 500.0;
        let base = (cpu_mcores / per_replica_target_m).ceil().max(1.0);
        let out = if p99_ms > 250.0 { base + 1.0 } else { base };
        debug!(
            metric="replicas_calc",
            cpu_mcores=%cpu_mcores,
            p99_ms=%p99_ms,
            per_replica_target_m=%per_replica_target_m,
            replicas=%out,
            "cpu-based replicas heuristic"
        );
        out
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum Aggregate {
    Avg,
    Max,
}

#[cfg(test)]
mod analyzer_tests {
    use super::Analyzer;

    #[test]
    fn replicas_heuristic_cpu_increases_with_cpu_and_latency() {
        let a = Analyzer::new();
        // 0 mcores -> 1 replica
        assert_eq!(a.heuristic_replicas_cpu(0.0, 0.0), 1.0);
        // 499m -> 1, 500m -> 1, 501m -> 2
        assert_eq!(a.heuristic_replicas_cpu(499.0, 0.0), 1.0);
        assert_eq!(a.heuristic_replicas_cpu(500.0, 0.0), 1.0);
        assert_eq!(a.heuristic_replicas_cpu(501.0, 0.0), 2.0);
        // High latency bumps by 1
        assert_eq!(a.heuristic_replicas_cpu(1000.0, 300.0), 3.0);
    }
}

fn parse_duration_secs(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    // Support simple forms: Ns, Nm, Nh (integer)
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num.parse().ok()?;
    match unit {
        "s" | "S" => Some(n),
        "m" | "M" => Some(n * 60),
        "h" | "H" => Some(n * 3600),
        _ => None,
    }
}

fn time_range_now(range_secs: u64) -> (i64, i64) {
    let end = chrono::Utc::now().timestamp();
    let start = end - (range_secs as i64);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_secs_parses_basic_units() {
        assert_eq!(parse_duration_secs("30s"), Some(30));
        assert_eq!(parse_duration_secs("10m"), Some(600));
        assert_eq!(parse_duration_secs("2h"), Some(7200));
        assert_eq!(parse_duration_secs(""), None);
        assert_eq!(parse_duration_secs("5x"), None);
    }
}
