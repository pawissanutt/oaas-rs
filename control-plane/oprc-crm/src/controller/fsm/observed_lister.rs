use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Api, ListParams};
use kube::core::{DynamicObject, GroupVersionKind};
use kube::discovery::ApiResource;
use kube::{Client, ResourceExt};

use super::observed::{ChildStatus, Observed};

fn owner_label_selector(name: &str) -> String {
    format!("oaas.io/owner={}", name)
}

/// Build an Observed snapshot of child resources owned by a ClassRuntime using the
/// well-known owner label. This is a best-effort view; failures to list a particular
/// kind will simply omit those children.
#[tracing::instrument(level = "debug", skip(client), fields(ns=%ns, class=%class_name))]
pub async fn observe_children(
    client: Client,
    ns: &str,
    class_name: &str,
    include_knative: bool,
) -> Observed {
    let mut obs = Observed {
        children: Vec::new(),
    };
    let lp = ListParams::default().labels(&owner_label_selector(class_name));

    // Deployments readiness: available_replicas > 0 (coarse)
    let dapi: Api<Deployment> = Api::namespaced(client.clone(), ns);
    if let Ok(list) = dapi.list(&lp).await {
        for d in list {
            let name = d.name_any();
            let ready = d
                .status
                .as_ref()
                .and_then(|s| s.available_replicas)
                .unwrap_or(0)
                > 0;
            obs.children.push(ChildStatus {
                name,
                kind: "Deployment".into(),
                ready,
                reason: None,
                message: None,
            });
        }
    }

    // Knative Services readiness via condition Ready=True
    if include_knative {
        // Attempt to list knative services; ignore errors if API absent
        let gvk = GroupVersionKind::gvk("serving.knative.dev", "v1", "Service");
        let ar = ApiResource::from_gvk(&gvk);
        let ks_api: Api<DynamicObject> =
            Api::namespaced_with(client.clone(), ns, &ar);
        if let Ok(list) = ks_api.list(&lp).await {
            for svc in list {
                let name = svc.name_any();
                let ready = svc
                    .data
                    .get("status")
                    .and_then(|s| s.get("conditions"))
                    .and_then(|c| c.as_array())
                    .and_then(|arr| {
                        arr.iter().find(|cond| {
                            cond.get("type").and_then(|v| v.as_str())
                                == Some("Ready")
                        })
                    })
                    .and_then(|cond| {
                        cond.get("status").and_then(|v| v.as_str())
                    })
                    .map(|s| s == "True")
                    .unwrap_or(false);
                obs.children.push(ChildStatus {
                    name,
                    kind: "KnativeService".into(),
                    ready,
                    reason: None,
                    message: None,
                });
            }
        }
    }

    obs
}
