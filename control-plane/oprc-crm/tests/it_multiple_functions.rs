// Integration test: verify controller renders one Deployment/Service per function in a DeploymentRecord


use k8s_openapi::api::{apps::v1::Deployment, core::v1::Service};
use kube::{
    Client,
    api::{Api, ListParams, PostParams},
};
use oprc_crm::crd::deployment_record::{
    DeploymentRecord, DeploymentRecordSpec, FunctionSpec,
};
// std::time::Duration not used in this test
mod common;
use common::{ControllerGuard, DIGITS, set_env, wait_for_cleanup_async};

#[test_log::test(tokio::test)]
#[ignore]
async fn controller_handles_multiple_functions_in_one_record() {
    // Arrange: disable prom/knative to force k8s Deployment/Service path
    let _g1 = set_env("OPRC_CRM_FEATURES_PROMETHEUS", "false");
    let _g2 = set_env("OPRC_CRM_FEATURES_KNATIVE", "false");
    // Ensure ODGM feature toggle is enabled for controller
    let _g3 = set_env("OPRC_CRM_FEATURES_ODGM", "true");

    let client = match Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIPPED: no Kubernetes context available: {}", e);
            return;
        }
    };

    let ns = "default";
    let name = format!("oaas-it-multi-{}", nanoid::nanoid!(6, &DIGITS));
    let guard = ControllerGuard::new(ns, &name, client.clone()).include_hpa();

    let api: Api<DeploymentRecord> = Api::namespaced(client.clone(), ns);
    let spec = DeploymentRecordSpec {
        selected_template: None,
        addons: Some(vec!["odgm".into()]),
        odgm_config: None,
        functions: vec![
            FunctionSpec {
                function_key: "fn-1".into(),
                replicas: 1,
                container_image: Some(
                    "ghcr.io/pawissanutt/oaas-rs/echo-fn:latest".into(),
                ),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: None,
                config: std::collections::HashMap::new(),
            },
            FunctionSpec {
                function_key: "fn-2".into(),
                replicas: 1,
                container_image: Some(
                    "ghcr.io/pawissanutt/oaas-rs/echo-fn:latest".into(),
                ),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: None,
                config: std::collections::HashMap::new(),
            },
        ],
        nfr_requirements: None,
        nfr: None,
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

    // Assert: two Deployments and two Services appear with owner label
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));

    let (dep_count, svc_count) = common::wait_for_children(
        ns,
        &name,
        client.clone(),
        2,
        2,
        30,
    )
    .await;

    assert!(
        dep_count >= 2,
        "expected at least 2 Deployments created by controller"
    );
    assert!(
        svc_count >= 2,
        "expected at least 2 Services created by controller"
    );

    // Also verify ODGM resources exist when addon enabled
    let deps = dep_api.list(&lp).await.expect("list deployments");
    let svcs = svc_api.list(&lp).await.expect("list services");
    let _odgm_dep = deps
        .items
        .iter()
        .find(|d| {
            d.metadata
                .name
                .as_ref()
                .map(|n| n.contains("-odgm") || n == &format!("{}-odgm", name))
                .unwrap_or(false)
        })
        .expect("expected odgm deployment");
    let _odgm_svc = svcs
        .items
        .iter()
        .find(|s| {
            s.metadata
                .name
                .as_ref()
                .map(|n| {
                    n.contains("-odgm") || n == &format!("{}-odgm-svc", name)
                })
                .unwrap_or(false)
        })
        .expect("expected odgm service");

    // Drop the controller guard so its Drop impl runs and starts cleanup,
    // then wait for cleanup to complete.
    drop(_guard);
    let _ = wait_for_cleanup_async(ns, &name, client.clone(), true, 30).await;
}
