#[test]
fn metrics_init_idempotent() {
    let m1 = oprc_observability::init_service_metrics("test-svc");
    m1.record_request();
    let m2 = oprc_observability::init_service_metrics("test-svc");
    m2.record_error();
    // success criteria: no panic
}

#[test]
fn otlp_metrics_skips_without_env() {
    // ensure the env var not set
    unsafe {
        std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "");
        std::env::set_var("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT", "");
    }
    let installed =
        oprc_observability::init_otlp_metrics_if_configured("svc").unwrap();
    assert!(
        !installed,
        "exporter should not install without endpoint env"
    );
}
