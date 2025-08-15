// Integration tests that expect a running k8s cluster with Prometheus Operator installed.
// Enable via: cargo test -p oprc-crm --test it_k8s -- --ignored

use kube::Client;
use oprc_crm::nfr::{PromOperatorProvider, ScrapeKind};
// no need for JoinHandle import; we spawn and ignore the handle
// no extra imports needed for helpers (provided by common)
mod common;
use common::{ensure_deployment_exists, ensure_service_exists, uniq};

// DNS-1123 safe suffix helper is provided by common::uniq

async fn get_client_and_provider() -> (Client, PromOperatorProvider) {
    let client = Client::try_default().await.expect("kube client");
    let provider = PromOperatorProvider::new(client.clone());
    assert!(
        provider.operator_crds_present().await,
        "prom operator CRDs missing"
    );
    (client, provider)
}

// resource creators are provided by common

// We mark the tests ignored by default; run explicitly when env is ready.
#[test_log::test(tokio::test)]
#[ignore]
async fn ensure_servicemonitor_in_cluster() {
    // Pre-conditions:
    // - KUBECONFIG points to a working cluster
    // - Prometheus Operator CRDs installed
    // - Namespace "default" exists

    let (client, provider) = get_client_and_provider().await;

    // Create a dummy Service to be selected by ServiceMonitor
    let ns = "default";
    let name = uniq("oaas-it-svc");
    let _svc_guard = ensure_service_exists(client.clone(), ns, &name).await;

    // Ensure ServiceMonitor
    let refs = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Service,
            &None,
            false,
        )
        .await
        .expect("ensure targets");

    assert!(refs.iter().any(|(k, _)| k == "ServiceMonitor"));
}

#[test_log::test(tokio::test)]
#[ignore]
async fn ensure_podmonitor_in_cluster() {
    let (client, provider) = get_client_and_provider().await;

    let ns = "default";
    let name = uniq("oaas-it-dep");

    let _dep_guard = ensure_deployment_exists(client.clone(), ns, &name).await;

    // Ensure PodMonitor
    let refs = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Pod,
            &None,
            false,
        )
        .await
        .expect("ensure targets");

    assert!(refs.iter().any(|(k, _)| k == "PodMonitor"));
}

#[test_log::test(tokio::test)]
#[ignore]
async fn ensure_servicemonitor_idempotent() {
    let (client, provider) = get_client_and_provider().await;

    let ns = "default";
    let name = uniq("oaas-it-svc2");
    let _svc_guard = ensure_service_exists(client.clone(), ns, &name).await;

    // First ensure
    let refs1 = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Service,
            &None,
            false,
        )
        .await
        .expect("ensure targets #1");
    assert!(refs1.iter().any(|(k, _)| k == "ServiceMonitor"));

    // Second ensure should be idempotent and succeed
    let refs2 = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Service,
            &None,
            false,
        )
        .await
        .expect("ensure targets #2");
    assert!(refs2.iter().any(|(k, _)| k == "ServiceMonitor"));
}

#[test_log::test(tokio::test)]
#[ignore]
async fn ensure_podmonitor_idempotent() {
    let (client, provider) = get_client_and_provider().await;

    let ns = "default";
    let name = uniq("oaas-it-dep2");

    let _dep_guard = ensure_deployment_exists(client.clone(), ns, &name).await;

    // First ensure
    let refs1 = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Pod,
            &None,
            false,
        )
        .await
        .expect("ensure targets #1");
    assert!(refs1.iter().any(|(k, _)| k == "PodMonitor"));

    // Second ensure should succeed as well
    let refs2 = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Pod,
            &None,
            false,
        )
        .await
        .expect("ensure targets #2");
    assert!(refs2.iter().any(|(k, _)| k == "PodMonitor"));
}

#[test_log::test(tokio::test)]
#[ignore]
async fn ensure_servicemonitor_with_extra_match_labels() {
    let (client, provider) = get_client_and_provider().await;

    let ns = "default";
    let name = uniq("oaas-it-svc3");
    let _svc_guard = ensure_service_exists(client.clone(), ns, &name).await;

    // Provide extra labels
    let extras = Some(vec![("team".to_string(), "core".to_string())]);
    let refs = provider
        .ensure_targets(
            ns,
            &name,
            &name,
            "oaas.io/owner",
            ScrapeKind::Service,
            &extras,
            false,
        )
        .await
        .expect("ensure targets");
    assert!(refs.iter().any(|(k, _)| k == "ServiceMonitor"));
}
