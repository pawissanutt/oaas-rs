use envconfig::Envconfig;
use std::sync::Arc;

use futures_util::StreamExt;
use k8s_openapi::api::{apps::v1::Deployment, core::v1::Service};
use kube::{
    Client, Resource, ResourceExt,
    api::{Api, DeleteParams, Patch, PatchParams},
    runtime::{Controller, controller::Action, watcher::Config},
};
use serde_json::json;
use tokio::time::Duration;
use tracing::{error, info};

use crate::config::CrmConfig;
use crate::crd::deployment_record::{
    Condition, ConditionStatus, ConditionType, DeploymentRecord,
    DeploymentRecordStatus,
};
use crate::templates::{RenderContext, TemplateManager};
use chrono::Utc;

#[derive(thiserror::Error, Debug)]
pub enum ReconcileErr {
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Clone)]
pub struct ControllerContext {
    pub client: Client,
    pub cfg: CrmConfig,
}

pub async fn run_controller(client: Client) -> anyhow::Result<()> {
    let api: Api<DeploymentRecord> = Api::all(client.clone());
    // Load config to honor feature flags during reconcile
    let cfg = CrmConfig::init_from_env()?.apply_profile_defaults();
    let ctx = Arc::new(ControllerContext {
        client: client.clone(),
        cfg,
    });

    Controller::new(api, Config::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|res| async move {
            match res {
                Ok((_obj_ref, action)) => {
                    info!("reconciled: requeue={:?}", action)
                }
                Err(e) => error!(error = ?e, "reconcile error"),
            }
        })
        .await;

    Ok(())
}

const FINALIZER: &str = "oaas.io/finalizer";

async fn reconcile(
    obj: Arc<DeploymentRecord>,
    ctx: Arc<ControllerContext>,
) -> Result<Action, ReconcileErr> {
    let ns = obj.namespace().unwrap_or_else(|| "default".to_string());
    let name = obj.name_any();
    let uid = obj.meta().uid.clone();

    let dr_api: Api<DeploymentRecord> =
        Api::namespaced(ctx.client.clone(), &ns);

    // Handle delete: cleanup children then remove finalizer
    if obj.meta().deletion_timestamp.is_some() {
        // Best-effort cleanup of children
        delete_children(&ctx.client, &ns, &name).await;
        return Ok(Action::await_change());
    }

    // Ensure finalizer
    if !obj
        .meta()
        .finalizers
        .as_ref()
        .map(|f| f.iter().any(|x| x == FINALIZER))
        .unwrap_or(false)
    {
        let mut finals = obj.meta().finalizers.clone().unwrap_or_default();
        finals.push(FINALIZER.to_string());
        let patch = json!({"metadata": {"finalizers": finals}});
        let _ = dr_api
            .patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
            .await
            .map_err(into_internal)?;
    }

    // Apply workload (Deployment + Service) with SSA
    let enable_odgm = ctx.cfg.features.odgm_sidecar.unwrap_or(false);
    apply_workload(&ctx.client, &ns, &name, uid.as_deref(), enable_odgm)
        .await?;

    // Events pending: will emit once Recorder wiring is finalized

    // Update status (observedGeneration, phase)
    let now = Utc::now().to_rfc3339();
    let status = json!({
        "status": DeploymentRecordStatus {
            phase: Some("Progressing".to_string()),
            message: Some("Applied workload".to_string()),
            observed_generation: obj.meta().generation,
            last_updated: Some(now.clone()),
            conditions: Some(vec![
                Condition { type_: ConditionType::Progressing, status: ConditionStatus::True, reason: Some("ApplySuccessful".into()), message: Some("Resources applied".into()), last_transition_time: Some(now) }
            ])
        }
    });
    let _ = dr_api
        .patch_status(&name, &PatchParams::default(), &Patch::Merge(&status))
        .await
        .map_err(into_internal)?;

    Ok(Action::requeue(Duration::from_secs(60)))
}

async fn apply_workload(
    client: &Client,
    ns: &str,
    name: &str,
    owner_uid: Option<&str>,
    enable_odgm_sidecar: bool,
) -> Result<(), ReconcileErr> {
    // Render manifests via TemplateManager
    let tm = TemplateManager::new();
    let (deployments, services) = tm.render_workload(RenderContext {
        name,
        owner_api_version: "oaas.io/v1alpha1",
        owner_kind: "DeploymentRecord",
        owner_uid,
        enable_odgm_sidecar,
        function_image: None,
        function_port: None,
        odgm_image: None,
        odgm_port: None,
        odgm_collections: None,
    });
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let pp = PatchParams::apply("oprc-crm").force();

    for dep in deployments {
        let dep_name = dep
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| name.to_string());
        let dep_json = serde_json::to_value(&dep).map_err(into_internal)?;
        let _ = dep_api
            .patch(&dep_name, &pp, &Patch::Apply(&dep_json))
            .await
            .map_err(into_internal)?;
    }

    for svc in services {
        let svc_name = svc
            .metadata
            .name
            .clone()
            .unwrap_or_else(|| format!("{}-svc", name));
        let svc_json = serde_json::to_value(&svc).map_err(into_internal)?;
        let _ = svc_api
            .patch(&svc_name, &pp, &Patch::Apply(&svc_json))
            .await
            .map_err(into_internal)?;
    }

    Ok(())
}

// owner_ref now provided and used inside TemplateManager

async fn delete_children(client: &Client, ns: &str, name: &str) {
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    // Function runtime resources
    let _ = dep_api.delete(name, &DeleteParams::default()).await;
    let _ = svc_api
        .delete(&format!("{}-svc", name), &DeleteParams::default())
        .await;

    // ODGM resources (separate workload)
    let odgm_name = format!("{}-odgm", name);
    let _ = dep_api.delete(&odgm_name, &DeleteParams::default()).await;
    let _ = svc_api
        .delete(&format!("{}-svc", odgm_name), &DeleteParams::default())
        .await;
}

fn into_internal<E: std::fmt::Display>(e: E) -> ReconcileErr {
    ReconcileErr::Internal(e.to_string())
}

fn error_policy(
    _obj: Arc<DeploymentRecord>,
    _error: &ReconcileErr,
    _ctx: Arc<ControllerContext>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}
