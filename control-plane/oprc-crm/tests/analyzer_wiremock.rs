use oprc_crm::nfr::Analyzer;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

// Helper to set an env var for the duration of a test and restore the previous value.
struct EnvGuard {
    key: &'static str,
    old: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(ref v) = self.old {
                std::env::set_var(self.key, v);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}

fn set_env_temp(key: &'static str, value: String) -> EnvGuard {
    let old = std::env::var(key).ok();
    unsafe {
        std::env::set_var(key, value);
    }
    EnvGuard { key, old }
}

// fn prom_query_response(value: f64) -> ResponseTemplate {
//     // Instant query style (vector)
//     let body = serde_json::json!({
//         "status": "success",
//         "data": {
//             "resultType": "vector",
//             "result": [ { "value": [ 0, value.to_string() ] } ]
//         }
//     });
//     ResponseTemplate::new(200).set_body_json(body)
// }

fn prom_range_response(values: &[f64]) -> ResponseTemplate {
    // Range query style (matrix) – create one series with provided values
    let mut vs_json = Vec::new();
    for (i, v) in values.iter().enumerate() {
        // (timestamp, value) pairs; timestamp not used in aggregation beyond parsing
        vs_json.push(serde_json::json!([i as u64, v.to_string()]));
    }
    let body = serde_json::json!({
        "status": "success",
        "data": {
            "resultType": "matrix",
            "result": [ { "values": vs_json } ]
        }
    });
    ResponseTemplate::new(200).set_body_json(body)
}

#[tokio::test]
async fn analyzer_observe_only_yields_replicas_cpu_memory() {
    let server = MockServer::start().await;

    // By default knative feature is enabled; Analyzer will issue range queries first.
    // Mock only the range queries so no instant fallback is performed.

    // RPS (knative activator_request_count) – not strictly asserted downstream, but needed when target_rps provided
    Mock::given(method("GET"))
        .and(path("/api/v1/query_range"))
        .and(query_param("query", "sum(rate(activator_request_count{namespace_name=\"ns\", configuration_name=\"demo\"}[1m]))"))
        .respond_with(prom_range_response(&[10.0, 12.0, 11.0])) // avg ~11 rps
        .expect(1)
        .mount(&server)
        .await;

    // p99 latency ms (knative activator_request_latencies_bucket)
    Mock::given(method("GET"))
        .and(path("/api/v1/query_range"))
        .and(query_param("query", "1000 * max(histogram_quantile(0.99, sum(rate(activator_request_latencies_bucket{namespace_name=\"ns\", configuration_name=\"demo\"}[1m])) by (revision_name, le)))"))
        .respond_with(prom_range_response(&[300.0, 295.0]))
        .expect(1)
        .mount(&server)
        .await;

    // CPU mcores
    Mock::given(method("GET"))
        .and(path("/api/v1/query_range"))
        .and(query_param("query", "1000 * sum(rate(container_cpu_usage_seconds_total{namespace=\"ns\", pod=~\"demo-.*\", container!=\"\"}[5m]))"))
        .respond_with(prom_range_response(&[200.0, 250.0, 225.0]))
        .expect(1)
        .mount(&server)
        .await;

    // Memory bytes
    Mock::given(method("GET"))
        .and(path("/api/v1/query_range"))
        .and(query_param("query", "sum(container_memory_working_set_bytes{namespace=\"ns\", pod=~\"demo-.*\", container!=\"\"})"))
        .respond_with(prom_range_response(&[1_000_000.0, 1_050_000.0]))
        .expect(1)
        .mount(&server)
        .await;

    let _guard = set_env_temp("OPRC_CRM_PROM_URL", server.uri());
    let analyzer = Analyzer::new();
    let recs = analyzer
        .observe_only("ns", "demo", Some(200.0), Some(500.0))
        .await
        .unwrap();

    // Should produce cpu & memory recommendations
    let dims: Vec<_> = recs.iter().map(|r| r.dimension.as_str()).collect();
    assert!(dims.contains(&"cpu"));
    assert!(dims.contains(&"memory"));
    assert!(dims.contains(&"replicas"));
}

#[tokio::test]
async fn analyzer_handles_empty_results_gracefully() {
    let server = MockServer::start().await;
    let empty = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": "success",
        "data": { "resultType": "vector", "result": [] }
    }));

    Mock::given(method("GET"))
        .and(path("/api/v1/query"))
        .respond_with(empty)
        .mount(&server)
        .await;

    let _guard = set_env_temp("OPRC_CRM_PROM_URL", server.uri());
    let analyzer = Analyzer::new();
    let recs = analyzer
        .observe_only("ns", "demo", None, Some(500.0))
        .await
        .unwrap();
    // With no data, may be empty or partial; must not error
    assert!(recs.len() <= 3);
}

#[tokio::test]
async fn analyzer_handles_server_errors_gracefully() {
    let server = MockServer::start().await;

    // Return 500 for any query
    let fail = ResponseTemplate::new(500);
    Mock::given(method("GET"))
        .and(path("/api/v1/query"))
        .respond_with(fail)
        .mount(&server)
        .await;

    let _guard = set_env_temp("OPRC_CRM_PROM_URL", server.uri());
    let analyzer = Analyzer::new();
    let recs = analyzer
        .observe_only("ns", "demo", None, None)
        .await
        .unwrap();
    // Should not crash; recommendations may be empty
    assert!(recs.is_empty());
}
