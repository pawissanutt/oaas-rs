// Kind-based integration tests for CRM covering single and multi-controller scenarios.
// These tests expect a running kind (or any KUBECONFIG) cluster with the DeploymentRecord CRD applied.
// Run explicitly: cargo test -p oprc-crm --test it_kind_multi -- --ignored --nocapture

#![allow(unused)]

use k8s_openapi::api::{apps::v1::Deployment, core::v1::Service};
use kube::{
    Client,
    api::{Api, ListParams, PostParams},
};
use oprc_crm::crd::class_runtime::{
    ClassRuntime as DeploymentRecord, ClassRuntimeSpec as DeploymentRecordSpec,
    FunctionSpec,
};
use tracing::debug;

mod common; // re-export test utils (ControllerGuard, DIGITS, etc.)
use common::{
    ControllerGuard, DIGITS, cleanup_k8s, set_env, uniq, wait_for_cleanup_async,
};
use k8s_openapi::api::core::v1::Pod;

/// Helper: create a DeploymentRecord in provided namespace with optional ODGM addon.
async fn create_dr(client: Client, ns: &str, name: &str, with_odgm: bool) {
    let api: Api<DeploymentRecord> = Api::namespaced(client, ns);
    let spec = DeploymentRecordSpec {
        selected_template: None,
        addons: if with_odgm {
            Some(vec!["odgm".to_string()])
        } else {
            None
        },
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
    };
    let dr = DeploymentRecord::new(name, spec);
    let _ = api
        .create(&PostParams::default(), &dr)
        .await
        .expect("create DR");
}

/// Single-controller happy path: verifies k8s Deployment + Service appear for one DR.
#[test_log::test(tokio::test)]
#[ignore]
async fn single_controller_happy_path() {
    // Enable ODGM addon + force plain k8s template path (disable prom/knative)
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    let _g3 = set_env("OPRC_CRM_FEATURES_ODGM", "true");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: kube client error: {e}");
            return;
        }
    };
    let ns = "default"; // rely on existing namespace
    let name = format!("oaas-it-kind-{}", nanoid::nanoid!(6, &DIGITS));
    let guard = ControllerGuard::new(ns, &name, client.clone());

    create_dr(client.clone(), ns, &name, true).await;

    // Spawn controller
    let ctrl_client = client.clone();
    let ctrl = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(ctrl_client).await;
    });
    let _guard = guard.with_controller(ctrl);

    // Poll for Deployment + Service with owner label
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));
    let mut found_dep = false;
    let mut found_svc = false;
    let mut found_odgm_dep = false;
    let mut found_odgm_svc = false;
    for _ in 0..50 {
        // up to ~50s
        if !found_dep {
            found_dep = dep_api
                .list(&lp)
                .await
                .map(|l| {
                    l.items.iter().any(|d| {
                        d.metadata
                            .name
                            .as_deref()
                            .map(|n| !n.ends_with("-odgm"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !found_svc {
            found_svc = svc_api
                .list(&lp)
                .await
                .map(|l| {
                    l.items.iter().any(|s| {
                        s.metadata
                            .name
                            .as_deref()
                            .map(|n| !n.ends_with("-odgm-svc"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !found_odgm_dep {
            found_odgm_dep = dep_api
                .list(&lp)
                .await
                .map(|l| {
                    l.items.iter().any(|d| {
                        d.metadata
                            .name
                            .as_deref()
                            .map(|n| n.ends_with("-odgm"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !found_odgm_svc {
            found_odgm_svc = svc_api
                .list(&lp)
                .await
                .map(|l| {
                    l.items.iter().any(|s| {
                        s.metadata
                            .name
                            .as_deref()
                            .map(|n| n.ends_with("-odgm-svc"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if found_dep && found_svc && found_odgm_dep && found_odgm_svc {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    assert!(found_dep, "expected Deployment created by controller");
    assert!(found_svc, "expected Service created by controller");
    assert!(found_odgm_dep, "expected ODGM Deployment created");
    assert!(found_odgm_svc, "expected ODGM Service created");

    // Explicit cleanup (in addition to ControllerGuard) to ensure pods removed before test exits.
    // Abort controller early to avoid it re-reconciling while we delete.
    drop(_guard); // triggers async cleanup
    // Wait for cleanup to complete for this owner
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 30).await;
}

/// Multi-controller scenario: run two controllers (in different logical namespaces) each reconciling its own DR.
/// We simulate multi-CRM by launching controllers with different namespaces via env override and distinct DR names.
/// This validates that controllers do not interfere and each set of children is created.
#[test_log::test(tokio::test)]
#[ignore]
async fn multi_controller_isolation() {
    // Disable optional features except enable ODGM for both controllers
    let _g_prom = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g_kn = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    let _g_odgm = set_env("OPRC_CRM_FEATURES_ODGM", "true");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: kube client error: {e}");
            return;
        }
    };

    // Use two namespaces; if they don't exist test will fail early (assume cluster prepared). Common choices: default, kube-system; safer to reuse default and create second ephemeral.
    // We'll attempt to create an ephemeral namespace for isolation; best-effort.
    let ns_a = "default";
    let ns_b = format!("oaas-it-ns-{}", nanoid::nanoid!(5, &DIGITS));
    // Create namespace B (ignore if exists)
    {
        use k8s_openapi::api::core::v1::Namespace;
        use kube::api::Api;
        use kube::api::PostParams;
        let ns_api: Api<Namespace> = Api::all(client.clone());
        let ns_obj = Namespace {
            metadata: kube::core::ObjectMeta {
                name: Some(ns_b.clone()),
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = ns_api
            .create(&PostParams::default(), &ns_obj)
            .await
            .or_else(|e| {
                if let kube::Error::Api(ae) = &e {
                    if ae.code == 409 {
                        return Ok(ns_obj.clone());
                    }
                }
                Err(e)
            });
    }

    let name_a = uniq("oaas-it-multi-a");
    let name_b = uniq("oaas-it-multi-b");

    // Create DRs in each namespace
    create_dr(client.clone(), ns_a, &name_a, true).await;
    create_dr(client.clone(), &ns_b, &name_b, true).await;

    // Spawn two controllers. They both watch all namespaces (CR is cluster-scoped); isolation is by owner label + ns.
    // NOTE: run_controller uses Api::all; we still ensure children appear in correct namespaces.
    let c1 = client.clone();
    let h1 = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(c1).await;
    });
    let c2 = client.clone();
    let h2 = tokio::spawn(async move {
        let _ = oprc_crm::controller::run_controller(c2).await;
    });

    // Validate both sets of children show up.
    let dep_api_a: Api<Deployment> = Api::namespaced(client.clone(), ns_a);
    let svc_api_a: Api<Service> = Api::namespaced(client.clone(), ns_a);
    let dep_api_b: Api<Deployment> = Api::namespaced(client.clone(), &ns_b);
    let svc_api_b: Api<Service> = Api::namespaced(client.clone(), &ns_b);
    let lp_a =
        ListParams::default().labels(&format!("oaas.io/owner={}", name_a));
    let lp_b =
        ListParams::default().labels(&format!("oaas.io/owner={}", name_b));
    let mut a_dep = false;
    let mut a_svc = false;
    let mut b_dep = false;
    let mut b_svc = false;
    let mut a_odgm_dep = false;
    let mut a_odgm_svc = false;
    let mut b_odgm_dep = false;
    let mut b_odgm_svc = false;
    for _ in 0..60 {
        // up to ~60s
        if !a_dep {
            a_dep = dep_api_a
                .list(&lp_a)
                .await
                .map(|l| {
                    l.items.iter().any(|d| {
                        d.metadata
                            .name
                            .as_deref()
                            .map(|n| !n.ends_with("-odgm"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !a_svc {
            a_svc = svc_api_a
                .list(&lp_a)
                .await
                .map(|l| {
                    l.items.iter().any(|s| {
                        s.metadata
                            .name
                            .as_deref()
                            .map(|n| !n.ends_with("-odgm-svc"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !a_odgm_dep {
            a_odgm_dep = dep_api_a
                .list(&lp_a)
                .await
                .map(|l| {
                    l.items.iter().any(|d| {
                        d.metadata
                            .name
                            .as_deref()
                            .map(|n| n.ends_with("-odgm"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !a_odgm_svc {
            a_odgm_svc = svc_api_a
                .list(&lp_a)
                .await
                .map(|l| {
                    l.items.iter().any(|s| {
                        s.metadata
                            .name
                            .as_deref()
                            .map(|n| n.ends_with("-odgm-svc"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !b_dep {
            b_dep = dep_api_b
                .list(&lp_b)
                .await
                .map(|l| {
                    l.items.iter().any(|d| {
                        d.metadata
                            .name
                            .as_deref()
                            .map(|n| !n.ends_with("-odgm"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !b_svc {
            b_svc = svc_api_b
                .list(&lp_b)
                .await
                .map(|l| {
                    l.items.iter().any(|s| {
                        s.metadata
                            .name
                            .as_deref()
                            .map(|n| !n.ends_with("-odgm-svc"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !b_odgm_dep {
            b_odgm_dep = dep_api_b
                .list(&lp_b)
                .await
                .map(|l| {
                    l.items.iter().any(|d| {
                        d.metadata
                            .name
                            .as_deref()
                            .map(|n| n.ends_with("-odgm"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if !b_odgm_svc {
            b_odgm_svc = svc_api_b
                .list(&lp_b)
                .await
                .map(|l| {
                    l.items.iter().any(|s| {
                        s.metadata
                            .name
                            .as_deref()
                            .map(|n| n.ends_with("-odgm-svc"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
        }
        if a_dep
            && a_svc
            && b_dep
            && b_svc
            && a_odgm_dep
            && a_odgm_svc
            && b_odgm_dep
            && b_odgm_svc
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    assert!(a_dep && a_svc, "expected children in namespace A");
    assert!(b_dep && b_svc, "expected children in namespace B");
    assert!(
        a_odgm_dep && a_odgm_svc,
        "expected ODGM children in namespace A"
    );
    assert!(
        b_odgm_dep && b_odgm_svc,
        "expected ODGM children in namespace B"
    );

    // Abort controllers and cleanup.
    h1.abort();
    h2.abort();

    // Explicit cleanup of each owner resources before deleting CRs/namespaces.
    cleanup_k8s(ns_a, &name_a, client.clone(), false).await;
    cleanup_k8s(&ns_b, &name_b, client.clone(), false).await;

    // Wait for pods to disappear in both namespaces.
    {
        use kube::api::{Api, ListParams};
        let pod_a: Api<Pod> = Api::namespaced(client.clone(), ns_a);
        let pod_b: Api<Pod> = Api::namespaced(client.clone(), &ns_b);
        let lp_a =
            ListParams::default().labels(&format!("oaas.io/owner={}", name_a));
        let lp_b =
            ListParams::default().labels(&format!("oaas.io/owner={}", name_b));
        // Wait for both namespaces cleanup via shared helper (best-effort)
        let _ =
            wait_for_cleanup_async(ns_a, &name_a, client.clone(), false, 40)
                .await;
        let _ =
            wait_for_cleanup_async(&ns_b, &name_b, client.clone(), false, 40)
                .await;
    }

    // Best-effort cleanup of created DRs & namespace B.
    use kube::api::DeleteParams;
    let dr_gvk = kube::core::GroupVersionKind::gvk(
        "oaas.io",
        "v1alpha1",
        "DeploymentRecord",
    );
    let ar = kube::discovery::ApiResource::from_gvk(&dr_gvk);
    let dyn_api_a: Api<kube::core::DynamicObject> =
        Api::namespaced_with(client.clone(), ns_a, &ar);
    let dyn_api_b: Api<kube::core::DynamicObject> =
        Api::namespaced_with(client.clone(), &ns_b, &ar);
    let _ = dyn_api_a.delete(&name_a, &DeleteParams::default()).await;
    let _ = dyn_api_b.delete(&name_b, &DeleteParams::default()).await;
    // Delete namespace B (ignore errors)
    {
        use k8s_openapi::api::core::v1::Namespace;
        let ns_api: Api<Namespace> = Api::all(client.clone());
        let _ = ns_api.delete(&ns_b, &DeleteParams::default()).await;
    }
}
