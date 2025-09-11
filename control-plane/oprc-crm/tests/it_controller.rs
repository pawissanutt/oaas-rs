// Integration tests require a running Kubernetes cluster. These tests are ignored by default.

use std::time::Duration;

use k8s_openapi::api::{apps::v1::Deployment, core::v1::Service};
use kube::{
    Client,
    api::{Api, ListParams, PostParams},
};
use oprc_crm::crd::class_runtime::{
    ClassRuntime as DeploymentRecord, ClassRuntimeSpec as DeploymentRecordSpec,
    FunctionSpec,
};
mod common;
use common::{ControllerGuard, DIGITS, set_env, wait_for_cleanup_async};

#[test_log::test(tokio::test)]
#[ignore]
async fn controller_deploys_k8s_deployment_and_service() {
    // Arrange: disable prom/knative to force k8s Deployment/Service path
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    let client = Client::try_default().await.expect("kube client");

    // Ensure our CRD exists (assumed applied by cluster setup recipe)
    let ns = "default";
    let name = format!("oaas-it-k8s-{}", nanoid::nanoid!(6, &DIGITS));
    let guard = ControllerGuard::new(ns, &name, client.clone());
    let api: Api<DeploymentRecord> = Api::namespaced(client.clone(), ns);
    let spec = DeploymentRecordSpec {
        selected_template: None,
        addons: None,
        odgm_config: None,
        functions: vec![FunctionSpec {
            function_key: "fn-1".into(),
            description: None,
            available_location: None,
            qos_requirement: None,
            provision_config: Some(oprc_models::ProvisionConfig {
                container_image: Some(
                    "ghcr.io/pawissanutt/oaas-rs/echo-fn:latest".into(),
                ),
                min_scale: Some(1),
                ..Default::default()
            }),
            config: std::collections::HashMap::new(),
        }],
        nfr_requirements: None,
        ..Default::default()
    };
    let dr = DeploymentRecord::new(&name, spec);
    let _ = api
        .create(&PostParams::default(), &dr)
        .await
        .expect("create DR");

    // Spawn controller in background
    let client_for_ctrl = client.clone();
    let ctrl = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(client_for_ctrl).await;
    });
    let _guard = guard.with_controller(ctrl);

    // Assert: Deployment and Service appear with owner label
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));

    let mut found_dep = false;
    let mut found_svc = false;
    for _ in 0..30 {
        // up to ~30s
        if !found_dep {
            found_dep = dep_api
                .list(&lp)
                .await
                .map(|l| !l.items.is_empty())
                .unwrap_or(false);
        }
        if !found_svc {
            found_svc = svc_api
                .list(&lp)
                .await
                .map(|l| !l.items.is_empty())
                .unwrap_or(false);
        }
        if found_dep && found_svc {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(found_dep, "expected Deployment created by controller");
    assert!(found_svc, "expected Service created by controller");

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 30).await;
}

#[test_log::test(tokio::test)]
#[ignore]
async fn controller_deploys_knative_service_when_available() {
    // Skip if no Knative CRDs present
    let client = Client::try_default().await.expect("kube client");
    if let Ok(discovery) =
        kube::discovery::Discovery::new(client.clone()).run().await
    {
        if !discovery
            .groups()
            .any(|g| g.name() == "serving.knative.dev")
        {
            eprintln!("knative CRDs not found; skipping test");
            return;
        }
    } else {
        eprintln!("discovery failed; skipping test");
        return;
    }

    // Enable knative, disable prom
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "true");

    let ns = "default";
    let name = format!("oaas-it-kn-{}", nanoid::nanoid!(6, &DIGITS));
    let guard = ControllerGuard::new(ns, &name, client.clone());
    let api: Api<DeploymentRecord> = Api::namespaced(client.clone(), ns);
    let spec = DeploymentRecordSpec {
        selected_template: None,
        addons: None,
        odgm_config: None,
        functions: vec![FunctionSpec {
            function_key: "fn-1".into(),
            description: None,
            available_location: None,
            qos_requirement: None,
            provision_config: Some(oprc_models::ProvisionConfig {
                container_image: Some(
                    "ghcr.io/pawissanutt/oaas-rs/echo-fn:latest".into(),
                ),
                min_scale: Some(1),
                ..Default::default()
            }),
            config: std::collections::HashMap::new(),
        }],
        nfr_requirements: None,
        ..Default::default()
    };
    let dr = DeploymentRecord::new(&name, spec);
    let _ = api
        .create(&PostParams::default(), &dr)
        .await
        .expect("create DR");

    // Spawn controller
    let client_for_ctrl = client.clone();
    let ctrl = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(client_for_ctrl).await;
    });
    let _guard = guard.with_controller(ctrl);

    // Wait for Knative Service (dynamic)
    let ar = kube::discovery::ApiResource::from_gvk(
        &kube::core::GroupVersionKind::gvk(
            "serving.knative.dev",
            "v1",
            "Service",
        ),
    );
    let dyn_api: Api<kube::core::DynamicObject> =
        Api::namespaced_with(client.clone(), ns, &ar);

    let mut found_kn = false;
    for _ in 0..30 {
        if dyn_api.get_opt(&name).await.unwrap_or(None).is_some() {
            found_kn = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(found_kn, "expected Knative Service created by controller");

    // Ensure no k8s Deployment (Knative path should not create one)
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));
    let empty = dep_api
        .list(&lp)
        .await
        .map(|l| l.items.is_empty())
        .unwrap_or(false);
    assert!(empty, "unexpected k8s Deployment when Knative enabled");

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 30).await;
}

#[test_log::test(tokio::test)]
#[ignore]
async fn controller_deletion_cleans_children_and_finalizer() {
    // Arrange: pure k8s path
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");

    let client = Client::try_default().await.expect("kube client");
    let ns = "default";
    let name = format!("oaas-it-del-{}", nanoid::nanoid!(6, &DIGITS));
    let guard = ControllerGuard::new(ns, &name, client.clone());

    let api: Api<DeploymentRecord> = Api::namespaced(client.clone(), ns);
    let spec = DeploymentRecordSpec {
        selected_template: None,
        addons: None,
        odgm_config: None,
        functions: vec![FunctionSpec {
            function_key: "fn-1".into(),
            description: None,
            available_location: None,
            qos_requirement: None,
            provision_config: Some(oprc_models::ProvisionConfig {
                container_image: Some(
                    "ghcr.io/pawissanutt/oaas-rs/echo-fn:latest".into(),
                ),
                port: None,
                max_concurrency: 0,
                need_http2: false,
                cpu_request: None,
                memory_request: None,
                cpu_limit: None,
                memory_limit: None,
                min_scale: Some(1),
                max_scale: None,
            }),
            config: std::collections::HashMap::new(),
        }],
        nfr_requirements: None,
        ..Default::default()
    };
    let dr = DeploymentRecord::new(&name, spec);
    let _ = api
        .create(&PostParams::default(), &dr)
        .await
        .expect("create DR");

    // Spawn controller
    let client_for_ctrl = client.clone();
    let ctrl = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(client_for_ctrl).await;
    });
    let _guard = guard.with_controller(ctrl);

    // Wait for children to exist
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));
    for _ in 0..30 {
        // up to ~30s
        let has_dep = dep_api
            .list(&lp)
            .await
            .map(|l| !l.items.is_empty())
            .unwrap_or(false);
        let has_svc = svc_api
            .list(&lp)
            .await
            .map(|l| !l.items.is_empty())
            .unwrap_or(false);
        if has_dep && has_svc {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    // Confirm finalizer present before delete
    let dr_got = api.get(&name).await.expect("get DR");
    let has_finalizer = dr_got
        .metadata
        .finalizers
        .as_ref()
        .map(|f| f.iter().any(|x| x == "oaas.io/finalizer"))
        .unwrap_or(false);
    assert!(has_finalizer, "controller should add finalizer");

    // Delete and ensure cleanup completes (finalizer removed -> resource fully deleted)
    let _ = api.delete(&name, &Default::default()).await;

    let mut gone = false;
    for _ in 0..60 {
        // up to ~60s
        let dr_opt = api.get_opt(&name).await.expect("get_opt DR");
        let dep_left = dep_api
            .list(&lp)
            .await
            .map(|l| l.items.len())
            .unwrap_or(999);
        let svc_left = svc_api
            .list(&lp)
            .await
            .map(|l| l.items.len())
            .unwrap_or(999);
        if dr_opt.is_none() && dep_left == 0 && svc_left == 0 {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(gone, "DR and its children should be fully removed");

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 60).await;
}

#[test_log::test(tokio::test)]
#[ignore]
async fn controller_deploys_odgm_deployment_and_service() {
    // Arrange: disable prom/knative to take plain k8s path; ODGM default-on
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: no Kubernetes context available: {}", e);
            return;
        }
    };
    let ns = "default";
    let name = format!("oaas-it-odgm-{}", nanoid::nanoid!(6, &DIGITS));
    let guard = ControllerGuard::new(ns, &name, client.clone());

    // Create DR (addons default provides odgm)
    let api: Api<DeploymentRecord> = Api::namespaced(client.clone(), ns);
    let dr = DeploymentRecord::new(
        &name,
        DeploymentRecordSpec {
            selected_template: None,
            addons: None, // default -> ["odgm"]
            odgm_config: None,
            functions: vec![FunctionSpec {
                function_key: "fn-1".into(),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: Some(oprc_models::ProvisionConfig {
                    container_image: Some("nginx:alpine".into()),
                    min_scale: Some(1),
                    ..Default::default()
                }),
                config: std::collections::HashMap::new(),
            }],
            nfr_requirements: None,
            ..Default::default()
        },
    );
    let _ = api
        .create(&PostParams::default(), &dr)
        .await
        .expect("create DR");

    // Spawn controller
    let client_for_ctrl = client.clone();
    let ctrl = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(client_for_ctrl).await;
    });
    let _guard = guard.with_controller(ctrl);

    // Poll for ODGM Deployment & Service (label-owned resource names ending with -odgm / -odgm-svc)
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));
    let mut have_odgm_dep = false;
    let mut have_odgm_svc = false;
    for _ in 0..45 {
        // up to ~45s
        if !have_odgm_dep {
            if let Ok(list) = dep_api.list(&lp).await {
                have_odgm_dep = list.items.iter().any(|d| {
                    d.metadata
                        .name
                        .as_deref()
                        .map(|n| {
                            n == name || n.starts_with(&format!("{}-", name))
                        })
                        .unwrap_or(false)
                });
            }
        }
        if !have_odgm_svc {
            if let Ok(list) = svc_api.list(&lp).await {
                have_odgm_svc = list.items.iter().any(|s| {
                    s.metadata
                        .name
                        .as_deref()
                        .map(|n| {
                            n == format!("{}-svc", name)
                                || n.starts_with(&format!("{}-", name))
                        })
                        .unwrap_or(false)
                });
            }
        }
        if have_odgm_dep && have_odgm_svc {
            break;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(
        have_odgm_dep,
        "expected ODGM Deployment created by controller"
    );
    assert!(have_odgm_svc, "expected ODGM Service created by controller");
    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 45).await;
}
