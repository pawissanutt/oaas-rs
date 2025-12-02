//! Controller module
//!
//! This module wires the Kubernetes controller for DeploymentRecords and
//! orchestrates three main components:
//! - Reconciler: applies/updates child resources (Deployment/Service, optional Knative),
//!   publishes events, and patches status/conditions. It also maintains a local
//!   in-memory cache of DeploymentRecords used by background loops.
//! - Analyzer loop: periodically observes metrics (via PromQL queries to VictoriaMetrics/Prometheus),
//!   computes observe-only recommendations, and patches them into DR status.
//! - Enforcer loop: when NFR enforcement is enabled and a replicas recommendation exists,
//!   enforces replicas via HPA.minReplicas if present, otherwise falls back to patching
//!   Deployment.spec.replicas (or Knative minScale). Stability and cooldown windows prevent
//!   thrashing. Successful enforcements emit NFRApplied events and update audit fields.
//!
//! Key conventions:
//! - Envconfig-driven configuration; see CrmConfig.
//! - Tracing is enabled across major branches.
//! - The reconciler uses SSA for desired child resources, but when enforcement of replicas
//!   is active it intentionally avoids owning Deployment.spec.replicas to prevent conflicts
//!   with the enforcer.
//! - A local DR cache (RwLock<HashMap>) avoids frequent list calls and feeds analyzer/enforcer.
//!
//! Public surface: `run_controller` starts the controller and background loops.

use envconfig::Envconfig;
use std::sync::Arc;

use futures_util::StreamExt;
use k8s_openapi::api::core::v1::ObjectReference;
use kube::runtime::events::Recorder;
use kube::{
    Client,
    runtime::{Controller, controller::Action, watcher::Config},
};
use tokio::time::Duration;
use tracing::{debug, error, info};

use crate::config::CrmConfig;
use crate::crd::class_runtime::ClassRuntime;

mod analyzer;
mod cache;
mod enforcer;
mod events;
pub mod fsm;
mod hpa_helper;
mod reconcile;
mod status;
mod status_reducer;

// Unit tests collocated in src tree
#[cfg(test)]
mod status_reducer_tests;
mod types;

pub use analyzer::analyzer_loop;
pub use enforcer::enforcer_loop;
pub use reconcile::reconcile;

#[derive(thiserror::Error, Debug)]
pub enum ReconcileErr {
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Clone)]
pub struct ControllerContext {
    pub client: Client,
    pub cfg: CrmConfig,
    pub include_knative: bool,
    pub analyzer: crate::nfr::Analyzer,
    // In-memory cache of DeploymentRecords keyed by "ns/name"
    pub dr_cache: cache::DeploymentRecordCache,
    pub event_recorder: Recorder,
}

#[tracing::instrument(level = "debug", skip(client))]
pub async fn run_controller(client: Client) -> anyhow::Result<()> {
    use crate::nfr::Analyzer;
    use kube::api::Api;
    use kube::discovery::Discovery;
    use kube::runtime::events::Reporter;

    // Load configuration and detect optional integrations
    let cfg = CrmConfig::init_from_env()?.apply_profile_defaults();

    // Scope controller to the configured Kubernetes namespace only
    let api: Api<ClassRuntime> =
        Api::namespaced(client.clone(), &cfg.k8s_namespace);

    // Knative discovery
    let mut have_knative = false;
    if cfg.features.knative {
        if let Ok(discovery) = Discovery::new(client.clone()).run().await {
            have_knative = discovery
                .groups()
                .any(|g| g.name() == "serving.knative.dev");
        }
    }
    let include_knative = have_knative && cfg.features.knative;
    debug!(include_knative, profile = %cfg.profile, "controller init");

    // Events recorder
    let reporter = Reporter {
        controller: "oprc-crm".into(),
        instance: None,
    };
    let recorder = Recorder::new(client.clone(), reporter);

    // Shared context
    let ctx = Arc::new(ControllerContext {
        client: client.clone(),
        cfg,
        include_knative,
        analyzer: Analyzer::new(),
        dr_cache: cache::DeploymentRecordCache::new(),
        event_recorder: recorder,
    });

    // Background loops - analyzer always runs if Prometheus URL is configured
    {
        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            analyzer_loop(ctx_clone).await;
        });
    }
    if ctx.cfg.features.nfr_enforcement.unwrap_or(false) {
        let ctx_clone = ctx.clone();
        tokio::spawn(async move {
            enforcer_loop(ctx_clone).await;
        });
    }

    // Main controller run
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

// Re-export utilities used by submodules
pub(crate) fn build_obj_ref(
    ns: &str,
    name: &str,
    uid: Option<&str>,
) -> ObjectReference {
    ObjectReference {
        kind: Some("ClassRuntime".into()),
        api_version: Some("oaas.io/v1alpha1".into()),
        name: Some(name.to_string()),
        namespace: Some(ns.to_string()),
        uid: uid.map(|u| u.to_string()),
        ..Default::default()
    }
}

pub(crate) fn into_internal<E: std::fmt::Display>(e: E) -> ReconcileErr {
    ReconcileErr::Internal(e.to_string())
}

fn error_policy(
    _obj: Arc<ClassRuntime>,
    _error: &ReconcileErr,
    _ctx: Arc<ControllerContext>,
) -> Action {
    Action::requeue(Duration::from_secs(60))
}
