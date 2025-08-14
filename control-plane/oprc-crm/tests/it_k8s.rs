// Feature-gated integration tests that expect a running k8s cluster with Prometheus Operator installed.
// Enable via: cargo test -p oprc-crm --features it-k8s --test it_k8s -- --ignored

#![cfg(feature = "it-k8s")]

use kube::{Client, api::{Api, PostParams}};
use oprc_crm::nfr::{PromOperatorProvider, ScrapeKind};
use k8s_openapi::api::core::v1::{Service, ServiceSpec, ServicePort, PodSpec, Container, ContainerPort, PodTemplateSpec};
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;

// DNS-1123 safe numeric suffix for unique names
const DIGITS: [char; 10] = ['0','1','2','3','4','5','6','7','8','9'];

fn uniq(prefix: &str) -> String { format!("{prefix}-{}", nanoid::nanoid!(6, &DIGITS)) }

// We mark the tests ignored by default; run explicitly when env is ready.
#[tokio::test]
#[ignore]
async fn ensure_servicemonitor_in_cluster() {
    // Pre-conditions:
    // - KUBECONFIG points to a working cluster
    // - Prometheus Operator CRDs installed
    // - Namespace "default" exists

    let client = Client::try_default().await.expect("kube client");
    let provider = PromOperatorProvider::new(client.clone());

    // Quickly verify CRDs presence
    assert!(provider.operator_crds_present().await, "prom operator CRDs missing");

    // Create a dummy Service to be selected by ServiceMonitor
    let ns = "default";
    let name = uniq("oaas-it-svc");
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);

    let svc = Service {
        metadata: kube::core::ObjectMeta {
            name: Some(name.clone()),
            labels: Some(std::collections::BTreeMap::from([
                ("oaas.io/owner".to_string(), name.clone()),
                ("app".to_string(), name.clone()),
            ])),
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: Some(std::collections::BTreeMap::from([
                ("oaas.io/owner".to_string(), name.clone()),
                ("app".to_string(), name.clone()),
            ])),
            ports: Some(vec![ServicePort { name: Some("http".into()), port: 8080, target_port: Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(8080)), ..Default::default() }]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let _ = svc_api.create(&PostParams::default(), &svc).await.or_else(|e| {
        if let kube::Error::Api(ae) = &e { if ae.code == 409 { return Ok(svc.clone()); } }
        Err(e)
    }).expect("create/get service");

    // Ensure ServiceMonitor
    let refs = provider.ensure_targets(
        ns,
        &name,
        &name,
        "oaas.io/owner",
        ScrapeKind::Service,
        &None,
        false,
    ).await.expect("ensure targets");

    assert!(refs.iter().any(|(k, _)| k == "ServiceMonitor"));
}

#[tokio::test]
#[ignore]
async fn ensure_podmonitor_in_cluster() {
    let client = Client::try_default().await.expect("kube client");
    let provider = PromOperatorProvider::new(client.clone());
    assert!(provider.operator_crds_present().await, "prom operator CRDs missing");

    let ns = "default";
    let name = uniq("oaas-it-dep");

    // Create a simple Deployment with labels to match
    let labels = std::collections::BTreeMap::from([
        ("oaas.io/owner".to_string(), name.clone()),
        ("app".to_string(), name.clone()),
    ]);

    let dep = Deployment {
        metadata: kube::core::ObjectMeta { name: Some(name.clone()), labels: Some(labels.clone()), ..Default::default() },
        spec: Some(DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector { match_labels: Some(labels.clone()), ..Default::default() },
            template: PodTemplateSpec {
                metadata: Some(kube::core::ObjectMeta { labels: Some(labels.clone()), ..Default::default() }),
                spec: Some(PodSpec {
                    containers: vec![Container {
                        name: "main".into(),
                        image: Some("nginx:alpine".into()),
                        ports: Some(vec![ContainerPort { container_port: 8080, ..Default::default() }]),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let _ = dep_api.create(&PostParams::default(), &dep).await.or_else(|e| {
        if let kube::Error::Api(ae) = &e { if ae.code == 409 { return Ok(dep.clone()); } }
        Err(e)
    }).expect("create/get deployment");

    // Ensure PodMonitor
    let refs = provider.ensure_targets(
        ns,
        &name,
        &name,
        "oaas.io/owner",
        ScrapeKind::Pod,
        &None,
        false,
    ).await.expect("ensure targets");

    assert!(refs.iter().any(|(k, _)| k == "PodMonitor"));
}

#[tokio::test]
#[ignore]
async fn ensure_servicemonitor_idempotent() {
    let client = Client::try_default().await.expect("kube client");
    let provider = PromOperatorProvider::new(client.clone());
    assert!(provider.operator_crds_present().await, "prom operator CRDs missing");

    let ns = "default";
    let name = uniq("oaas-it-svc2");
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);

    let svc = Service {
        metadata: kube::core::ObjectMeta {
            name: Some(name.clone()),
            labels: Some(std::collections::BTreeMap::from([
                ("oaas.io/owner".to_string(), name.clone()),
                ("app".to_string(), name.clone()),
            ])),
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: Some(std::collections::BTreeMap::from([
                ("oaas.io/owner".to_string(), name.clone()),
                ("app".to_string(), name.clone()),
            ])),
            ports: Some(vec![ServicePort { name: Some("http".into()), port: 8080, target_port: Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(8080)), ..Default::default() }]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let _ = svc_api.create(&PostParams::default(), &svc).await.or_else(|e| {
        if let kube::Error::Api(ae) = &e { if ae.code == 409 { return Ok(svc.clone()); } }
        Err(e)
    }).expect("create/get service");

    // First ensure
    let refs1 = provider.ensure_targets(
        ns,
        &name,
        &name,
        "oaas.io/owner",
        ScrapeKind::Service,
        &None,
        false,
    ).await.expect("ensure targets #1");
    assert!(refs1.iter().any(|(k, _)| k == "ServiceMonitor"));

    // Second ensure should be idempotent and succeed
    let refs2 = provider.ensure_targets(
        ns,
        &name,
        &name,
        "oaas.io/owner",
        ScrapeKind::Service,
        &None,
        false,
    ).await.expect("ensure targets #2");
    assert!(refs2.iter().any(|(k, _)| k == "ServiceMonitor"));
}

#[tokio::test]
#[ignore]
async fn ensure_podmonitor_idempotent() {
    let client = Client::try_default().await.expect("kube client");
    let provider = PromOperatorProvider::new(client.clone());
    assert!(provider.operator_crds_present().await, "prom operator CRDs missing");

    let ns = "default";
    let name = uniq("oaas-it-dep2");

    // Create a simple Deployment to be matched by PodMonitor
    let labels = std::collections::BTreeMap::from([
        ("oaas.io/owner".to_string(), name.clone()),
        ("app".to_string(), name.clone()),
    ]);

    let dep = Deployment {
        metadata: kube::core::ObjectMeta { name: Some(name.clone()), labels: Some(labels.clone()), ..Default::default() },
        spec: Some(DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector { match_labels: Some(labels.clone()), ..Default::default() },
            template: PodTemplateSpec {
                metadata: Some(kube::core::ObjectMeta { labels: Some(labels.clone()), ..Default::default() }),
                spec: Some(PodSpec { containers: vec![Container { name: "main".into(), image: Some("nginx:alpine".into()), ports: Some(vec![ContainerPort { container_port: 8080, ..Default::default() }]), ..Default::default() }], ..Default::default() }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };

    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let _ = dep_api.create(&PostParams::default(), &dep).await.or_else(|e| {
        if let kube::Error::Api(ae) = &e { if ae.code == 409 { return Ok(dep.clone()); } }
        Err(e)
    }).expect("create/get deployment");

    // First ensure
    let refs1 = provider.ensure_targets(ns, &name, &name, "oaas.io/owner", ScrapeKind::Pod, &None, false).await.expect("ensure targets #1");
    assert!(refs1.iter().any(|(k, _)| k == "PodMonitor"));

    // Second ensure should succeed as well
    let refs2 = provider.ensure_targets(ns, &name, &name, "oaas.io/owner", ScrapeKind::Pod, &None, false).await.expect("ensure targets #2");
    assert!(refs2.iter().any(|(k, _)| k == "PodMonitor"));
}

#[tokio::test]
#[ignore]
async fn ensure_servicemonitor_with_extra_match_labels() {
    let client = Client::try_default().await.expect("kube client");
    let provider = PromOperatorProvider::new(client.clone());
    assert!(provider.operator_crds_present().await, "prom operator CRDs missing");

    let ns = "default";
    let name = uniq("oaas-it-svc3");
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);

    // Create a service
    let svc = Service {
        metadata: kube::core::ObjectMeta {
            name: Some(name.clone()),
            labels: Some(std::collections::BTreeMap::from([
                ("oaas.io/owner".to_string(), name.clone()),
                ("app".to_string(), name.clone()),
            ])),
            ..Default::default()
        },
        spec: Some(ServiceSpec {
            selector: Some(std::collections::BTreeMap::from([
                ("oaas.io/owner".to_string(), name.clone()),
                ("app".to_string(), name.clone()),
            ])),
            ports: Some(vec![ServicePort { name: Some("http".into()), port: 8080, target_port: Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(8080)), ..Default::default() }]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let _ = svc_api.create(&PostParams::default(), &svc).await.or_else(|e| {
        if let kube::Error::Api(ae) = &e { if ae.code == 409 { return Ok(svc.clone()); } }
        Err(e)
    }).expect("create/get service");

    // Provide extra labels
    let extras = Some(vec![("team".to_string(), "core".to_string())]);
    let refs = provider.ensure_targets(ns, &name, &name, "oaas.io/owner", ScrapeKind::Service, &extras, false).await.expect("ensure targets");
    assert!(refs.iter().any(|(k, _)| k == "ServiceMonitor"));
}
