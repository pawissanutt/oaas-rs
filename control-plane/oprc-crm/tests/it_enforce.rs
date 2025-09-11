// Integration tests require a running Kubernetes cluster. These tests are ignored by default.

use std::time::Duration;

use kube::{
    Client,
    api::{Api, ListParams, Patch, PatchParams, PostParams},
};

use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::autoscaling::v2 as autoscalingv2;
use k8s_openapi::api::core::v1::Event;

use oprc_crm::crd::class_runtime::{
    ClassRuntime, ClassRuntimeSpec, NfrEnforcementSpec,
};
use oprc_crm::nfr::PromOperatorProvider;

mod common;
use common::{
    ControllerGuard, DIGITS, set_env, uniq, wait_for_cleanup_async,
    wait_for_deployment,
};

#[test_log::test(tokio::test)]
#[ignore]
async fn enforce_hpa_minreplicas_when_hpa_present() {
    // Arrange fast ticks and enable enforcement + HPA; disable prom/knative
    let _g0 = set_env("OPRC_CRM_ANALYZER_INTERVAL_SECS", "1");
    let _g1 = set_env("OPRC_CRM_ENFORCEMENT_STABILITY_SECS", "1");
    let _g2 = set_env("OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS", "1");
    let _g3 = set_env("OPRC_CRM_FEATURES_NFR_ENFORCEMENT", "true");
    let _g4 = set_env("OPRC_CRM_FEATURES_HPA", "true");
    let _g5 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    let _g6 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g7 = set_env("OPRC_CRM_FEATURES_ODGM", "true");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: no Kubernetes context available: {}", e);
            return;
        }
    };
    let ns = "default";
    let name = uniq("oaas-it-enf-hpa");
    // Setup cleanup guard early so it runs even on failure
    let guard = ControllerGuard::new(ns, &name, client.clone()).include_hpa();

    // Create DR with enforce mode and replicas dimension
    let api: Api<ClassRuntime> = Api::namespaced(client.clone(), ns);
    let dr = ClassRuntime::new(
        &name,
        ClassRuntimeSpec {
            class_key: Some("class-1".into()),
            selected_template: Some("dev".into()),
            addons: Some(vec!["odgm".into()]),
            odgm_config: None,
            functions: vec![oprc_crm::crd::class_runtime::FunctionSpec {
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
            enforcement: Some(NfrEnforcementSpec {
                mode: Some("enforce".into()),
                dimensions: Some(vec!["replicas".into()]),
            }),
        },
    );
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

    // Wait for controller to apply workload (Deployment present)
    wait_for_deployment(ns, &name, client.clone()).await;

    // Create an HPA targeting the Deployment with min=1, max=10
    let hpa_api: Api<autoscalingv2::HorizontalPodAutoscaler> =
        Api::namespaced(client.clone(), ns);
    let hpa = autoscalingv2::HorizontalPodAutoscaler {
        metadata: kube::core::ObjectMeta {
            name: Some(name.clone()),
            ..Default::default()
        },
        spec: Some(autoscalingv2::HorizontalPodAutoscalerSpec {
            scale_target_ref: autoscalingv2::CrossVersionObjectReference {
                api_version: Some("apps/v1".into()),
                kind: "Deployment".into(),
                name: name.clone(),
            },
            min_replicas: Some(1),
            max_replicas: 10,
            ..Default::default()
        }),
        ..Default::default()
    };
    let _ = hpa_api
        .create(&PostParams::default(), &hpa)
        .await
        .or_else(|e| {
            if let kube::Error::Api(ae) = &e {
                if ae.code == 409 {
                    return Ok(hpa.clone());
                }
            }
            Err(e)
        })
        .expect("create/get HPA");

    // Patch DR status with a replicas recommendation target=3
    // Chart CRD expects nfr_recommendations as an object; send as { replicas: <n> }
    let patch = serde_json::json!({
        "status": {
            "nfr_recommendations": { "replicas": 3 }
        }
    });
    let _ = api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await
        .expect("patch status");
    // Force a reconcile to refresh CRM's local cache by tweaking a metadata annotation
    let bump = serde_json::json!({
        "metadata": { "annotations": { "enf-test/bump": nanoid::nanoid!(6, &DIGITS) } }
    });
    let _ = api
        .patch(&name, &PatchParams::default(), &Patch::Merge(&bump))
        .await
        .expect("bump metadata to trigger reconcile");

    // Assert: HPA.minReplicas updated to 3 by enforcer; ensure within ~45s
    let mut ok = false;
    for _ in 0..45 {
        if let Some(curr) = hpa_api.get_opt(&name).await.unwrap_or(None) {
            let got =
                curr.spec.as_ref().and_then(|s| s.min_replicas).unwrap_or(0);
            if got == 3 {
                ok = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(ok, "expected HPA.minReplicas=3 to be applied");

    // Assert: NFRApplied event exists for this DR
    let ev_api: Api<Event> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().fields(&format!(
        "involvedObject.kind=ClassRuntime,involvedObject.name={}",
        name
    ));
    let mut have_event = false;
    for _ in 0..10 {
        // up to ~10s
        if let Ok(list) = ev_api.list(&lp).await {
            have_event = list
                .items
                .iter()
                .any(|e| e.reason.as_deref() == Some("NFRApplied"));
            if have_event {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(have_event, "expected NFRApplied event to be published");

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), true, 30).await;
}

#[test_log::test(tokio::test)]
#[ignore]
async fn enforce_fallback_updates_deployment_when_hpa_absent() {
    // Arrange: enforcement on, HPA feature on but no HPA created; fast ticks
    let _g0 = set_env("OPRC_CRM_ANALYZER_INTERVAL_SECS", "1");
    let _g1 = set_env("OPRC_CRM_ENFORCEMENT_STABILITY_SECS", "1");
    let _g2 = set_env("OPRC_CRM_ENFORCEMENT_COOLDOWN_SECS", "1");
    let _g3 = set_env("OPRC_CRM_FEATURES_NFR_ENFORCEMENT", "true");
    let _g4 = set_env("OPRC_CRM_FEATURES_HPA", "true");
    let _g5 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    let _g6 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g7 = set_env("OPRC_CRM_FEATURES_ODGM", "true");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: no Kubernetes context available: {}", e);
            return;
        }
    };
    let ns = "default";
    let name = uniq("oaas-it-enf-fb");
    let guard = ControllerGuard::new(ns, &name, client.clone());

    let api: Api<ClassRuntime> = Api::namespaced(client.clone(), ns);
    let dr = ClassRuntime::new(
        &name,
        ClassRuntimeSpec {
            class_key: Some("class-1".into()),
            selected_template: Some("dev".into()),
            addons: Some(vec!["odgm".into()]),
            odgm_config: None,
            functions: vec![oprc_crm::crd::class_runtime::FunctionSpec {
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
            enforcement: Some(NfrEnforcementSpec {
                mode: Some("enforce".into()),
                dimensions: Some(vec!["replicas".into()]),
            }),
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

    // Wait for Deployment then patch status with replicas recommendation
    wait_for_deployment(ns, &name, client.clone()).await;
    // Chart CRD expects nfr_recommendations as an object; send as { replicas: <n> }
    let patch = serde_json::json!({
        "status": {
            "nfr_recommendations": { "replicas": 4 }
        }
    });
    let _ = api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&patch))
        .await
        .expect("patch status");
    // Force a reconcile to refresh local cache by tweaking a metadata annotation
    let bump = serde_json::json!({
        "metadata": { "annotations": { "enf-test/bump": nanoid::nanoid!(6, &DIGITS) } }
    });
    let _ = api
        .patch(&name, &PatchParams::default(), &Patch::Merge(&bump))
        .await
        .expect("bump metadata to trigger reconcile");

    // Assert: Deployment.spec.replicas becomes 4 (fallback path)
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let mut ok = false;
    for i in 0..75 {
        // allow more headroom for timing
        if let Some(dep) = dep_api.get_opt(&name).await.unwrap_or(None) {
            let got = dep.spec.as_ref().and_then(|s| s.replicas).unwrap_or(0);
            tracing::debug!(attempt = i, got, "polling deployment replicas");
            if got == 4 {
                ok = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(ok, "expected Deployment.spec.replicas=4 to be applied");

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 30).await;
}

#[test_log::test(tokio::test)]
#[ignore]
async fn status_has_prometheus_disabled_condition_when_crds_missing() {
    // Enable prometheus feature to exercise condition path
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "true");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    let _g3 = set_env("OPRC_CRM_FEATURES_ODGM", "true");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: no Kubernetes context available: {}", e);
            return;
        }
    };
    let provider = PromOperatorProvider::new(client.clone());
    if provider.operator_crds_present().await {
        eprintln!(
            "Prometheus Operator CRDs present; skipping PrometheusDisabled test"
        );
        return;
    }

    let ns = "default";
    let name = uniq("oaas-it-prom-disabled");
    let guard = ControllerGuard::new(ns, &name, client.clone());
    let api: Api<ClassRuntime> = Api::namespaced(client.clone(), ns);
    let dr = ClassRuntime::new(
        &name,
        ClassRuntimeSpec {
            selected_template: Some("dev".into()),
            addons: Some(vec!["odgm".into()]),
            odgm_config: None,
            functions: vec![oprc_crm::crd::class_runtime::FunctionSpec {
                function_key: "fn-1".into(),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: Some(oprc_models::ProvisionConfig {
                    container_image: Some("nginx:alpine".into()),
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

    // Wait until controller reconciles and sets status; then check condition
    let mut has_cond = false;
    for _ in 0..30 {
        if let Some(curr) = api.get_opt(&name).await.unwrap_or(None) {
            if let Some(st) = curr.status {
                if let Some(conds) = st.conditions {
                    has_cond = conds.iter().any(|c| {
                        c.reason.as_deref() == Some("PrometheusDisabled")
                    });
                    if has_cond {
                        break;
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    assert!(
        has_cond,
        "expected PrometheusDisabled condition when CRDs are missing"
    );

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), false, 30).await;
}
