use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{Service, ServicePort};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::core::DynamicObject;
use kube::discovery::ApiResource;
use kube::{
    Client,
    api::{Api, ListParams, Patch, PatchParams, PostParams},
};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::thread;
use tokio::task::JoinHandle;

pub const DIGITS: [char; 10] =
    ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
pub fn uniq(prefix: &str) -> String {
    format!("{prefix}-{}", nanoid::nanoid!(6, &DIGITS))
}

pub struct EnvGuard {
    key: &'static str,
    old: Option<String>,
}
impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(ref v) = self.old {
                std::env::set_var(self.key, v);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
pub fn set_env(key: &'static str, val: &str) -> EnvGuard {
    let old = std::env::var(key).ok();
    unsafe {
        std::env::set_var(key, val);
    }
    EnvGuard { key, old }
}

pub async fn wait_for_deployment(ns: &str, name: &str, client: Client) {
    let dep: Api<Deployment> = Api::namespaced(client, ns);
    for _ in 0..60 {
        if dep.get_opt(name).await.unwrap_or(None).is_some() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    }
    panic!("deployment {}/{} not found in time", ns, name);
}

/// Wait until the controller-created children (Deployments and Services) for a
/// DeploymentRecord reach the specified minimum counts or the timeout expires.
/// Returns (deployments_found, services_found) on success, or the last seen
/// counts when the timeout elapses.
pub async fn wait_for_children(
    ns: &str,
    name: &str,
    client: Client,
    min_deployments: usize,
    min_services: usize,
    timeout_secs: u64,
) -> (usize, usize) {
    let dep: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));

    let mut dep_count = 0usize;
    let mut svc_count = 0usize;
    let mut elapsed = 0u64;
    while elapsed < timeout_secs {
        if let Ok(list) = dep.list(&lp).await {
            dep_count = list.items.len();
        }
        if let Ok(list) = svc.list(&lp).await {
            svc_count = list.items.len();
        }
        if dep_count >= min_deployments && svc_count >= min_services {
            return (dep_count, svc_count);
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        elapsed += 1;
    }
    (dep_count, svc_count)
}

/// Like `wait_for_children` but returns the full lists of Deployments and Services
/// observed when the condition was met (or at timeout). Useful for richer asserts.
pub async fn wait_for_children_full(
    ns: &str,
    name: &str,
    client: Client,
    min_deployments: usize,
    min_services: usize,
    timeout_secs: u64,
) -> (Vec<Deployment>, Vec<Service>) {
    let dep: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));

    let mut dep_list: Vec<Deployment> = Vec::new();
    let mut svc_list: Vec<Service> = Vec::new();
    let mut elapsed = 0u64;
    while elapsed < timeout_secs {
        if let Ok(list) = dep.list(&lp).await {
            dep_list = list.items.clone();
        }
        if let Ok(list) = svc.list(&lp).await {
            svc_list = list.items.clone();
        }
        if dep_list.len() >= min_deployments && svc_list.len() >= min_services {
            return (dep_list, svc_list);
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        elapsed += 1;
    }
    (dep_list, svc_list)
}

/// Poll until the dynamic DeploymentRecord and its children are gone, or the
/// timeout elapses. This is the async part executed inside a helper thread.
pub async fn wait_for_cleanup_async(
    ns: &str,
    name: &str,
    client: Client,
    include_hpa: bool,
    timeout_secs: u64,
) {
    use kube::core::DynamicObject;
    use kube::discovery::ApiResource;

    let dep: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));
    let dyn_gvk = kube::core::GroupVersionKind::gvk(
        "oaas.io",
        "v1alpha1",
        "DeploymentRecord",
    );
    let ar = ApiResource::from_gvk(&dyn_gvk);
    let dyn_api: Api<DynamicObject> =
        Api::namespaced_with(client.clone(), ns, &ar);

    let mut elapsed = 0u64;
    while elapsed < timeout_secs {
        // Check for DeploymentRecord presence
        let dr_present = dyn_api.get_opt(name).await.unwrap_or(None).is_some();

        // Check for any children left
        let dep_count = dep.list(&lp).await.map(|l| l.items.len()).unwrap_or(0);
        let svc_count = svc.list(&lp).await.map(|l| l.items.len()).unwrap_or(0);
        let mut hpa_present = false;
        if include_hpa {
            let hpa: Api<
                k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler,
            > = Api::namespaced(client.clone(), ns);
            hpa_present = hpa.get_opt(name).await.unwrap_or(None).is_some();
        }

        if !dr_present && dep_count == 0 && svc_count == 0 && !hpa_present {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        elapsed += 1;
    }
}

pub async fn cleanup_k8s(
    ns: &str,
    name: &str,
    client: Client,
    include_hpa: bool,
) {
    let dep: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));
    if let Ok(list) = dep.list(&lp).await {
        for d in list {
            let _ = dep
                .delete(
                    &d.metadata.name.unwrap_or_else(|| name.to_string()),
                    &Default::default(),
                )
                .await;
        }
    }
    if let Ok(list) = svc.list(&lp).await {
        for s in list {
            let _ = svc
                .delete(
                    &s.metadata.name.unwrap_or_else(|| format!("{}-svc", name)),
                    &Default::default(),
                )
                .await;
        }
    }
    if include_hpa {
        let hpa: Api<
            k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler,
        > = Api::namespaced(client.clone(), ns);
        let _ = hpa.delete(name, &Default::default()).await;
    }

    // Also attempt to remove the DeploymentRecord CR (dynamic) including finalizer.
    // This avoids leaking CR instances between tests when the controller was aborted early.
    let gvk = kube::core::GroupVersionKind::gvk(
        "oaas.io",
        "v1alpha1",
        "DeploymentRecord",
    );
    let ar = ApiResource::from_gvk(&gvk);
    let dyn_api: Api<kube::core::DynamicObject> =
        Api::namespaced_with(client.clone(), ns, &ar);
    if let Ok(Some(dr)) = dyn_api.get_opt(name).await {
        // If finalizers present, patch them away first to allow deletion to complete.
        if dr
            .metadata
            .finalizers
            .as_ref()
            .map(|f| !f.is_empty())
            .unwrap_or(false)
        {
            let patch = json!({"metadata": {"finalizers": []}});
            let _ = dyn_api
                .patch(name, &PatchParams::default(), &Patch::Merge(&patch))
                .await;
        }
        let _ = dyn_api.delete(name, &Default::default()).await;
    }

    // Dynamic cleanup for optional children the controller might have created:
    // - Knative Service (serving.knative.dev/v1 Service)
    // - ServiceMonitor / PodMonitor (monitoring.coreos.com/v1)
    async fn try_dynamic_delete(
        client: Client,
        ns: &str,
        name: &str,
        group: &str,
        version: &str,
        kind: &str,
    ) {
        let gvk = kube::core::GroupVersionKind::gvk(group, version, kind);
        let ar = ApiResource::from_gvk(&gvk);
        let api: Api<DynamicObject> = Api::namespaced_with(client, ns, &ar);
        let _ = api.delete(name, &Default::default()).await;
    }
    // Fire-and-forget deletions
    let c1 = client.clone();
    let c2 = client.clone();
    let c3 = client.clone();
    let futs = [
        try_dynamic_delete(
            c1,
            ns,
            name,
            "serving.knative.dev",
            "v1",
            "Service",
        ),
        try_dynamic_delete(
            c2,
            ns,
            name,
            "monitoring.coreos.com",
            "v1",
            "ServiceMonitor",
        ),
        try_dynamic_delete(
            c3,
            ns,
            name,
            "monitoring.coreos.com",
            "v1",
            "PodMonitor",
        ),
    ];
    // Run concurrently; ignore results
    for f in futs {
        let _ = f.await;
    }
}

pub async fn delete_named<T>(ns: &str, name: &str, client: Client)
where
    T: k8s_openapi::Resource<Scope = k8s_openapi::NamespaceResourceScope>
        + k8s_openapi::Metadata<Ty = kube::core::ObjectMeta>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + 'static,
{
    let api: Api<T> = Api::namespaced(client, ns);
    let _ = api.delete(name, &Default::default()).await;
}

pub struct ControllerGuard {
    ns: String,
    name: String,
    client: Client,
    ctrl: Option<JoinHandle<()>>,
    include_hpa: bool,
}
impl ControllerGuard {
    pub fn new(ns: &str, name: &str, client: Client) -> Self {
        Self {
            ns: ns.to_string(),
            name: name.to_string(),
            client,
            ctrl: None,
            include_hpa: false,
        }
    }
    pub fn with_controller(mut self, ctrl: JoinHandle<()>) -> Self {
        self.ctrl = Some(ctrl);
        self
    }
    pub fn include_hpa(mut self) -> Self {
        self.include_hpa = true;
        self
    }
}
impl Drop for ControllerGuard {
    fn drop(&mut self) {
        if let Some(ref h) = self.ctrl {
            h.abort();
        }
        let ns = self.ns.clone();
        let name = self.name.clone();
        let client = self.client.clone();
        let include_hpa = self.include_hpa;

        // Fire the best-effort immediate deletions (same as before)
        let _ = tokio::spawn(async move {
            cleanup_k8s(&ns, &name, client.clone(), include_hpa).await;
        });

        // Now actively wait for cleanup to complete on a helper thread. We don't
        // want to block the current Tokio reactor, so spawn a std thread that
        // creates a tiny runtime to perform the async polling.
        let ns2 = self.ns.clone();
        let name2 = self.name.clone();
        let client2 = self.client.clone();
        let include_hpa2 = self.include_hpa;
        // Channel to receive completion signal (or timeout)
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            // Create a minimal runtime for the polling; ignore errors.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("create temp rt");
            let done = rt.block_on(async move {
                // Wait up to 15s for cleanup
                wait_for_cleanup_async(&ns2, &name2, client2, include_hpa2, 15)
                    .await;
            });
            // Notify caller the wait finished (best-effort)
            let _ = tx.send(());
            // drop runtime result value
            let _ = done;
        });
        // Wait up to 16s for the helper to signal completion; don't panic on timeout
        let _ = rx.recv_timeout(std::time::Duration::from_secs(16));
    }
}

pub fn default_labels(name: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("oaas.io/owner".to_string(), name.to_string()),
        ("app".to_string(), name.to_string()),
    ])
}

pub async fn ensure_service_exists(
    client: Client,
    ns: &str,
    name: &str,
) -> K8sCleanupGuard<Service> {
    let svc: Api<Service> = Api::namespaced(client.clone(), ns);
    let labels = default_labels(name);
    let obj = Service {
        metadata: kube::core::ObjectMeta {
            name: Some(name.to_string()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        spec: Some(k8s_openapi::api::core::v1::ServiceSpec {
            selector: Some(labels),
            ports: Some(vec![ServicePort {
                name: Some("http".into()),
                port: 8080,
                target_port: Some(IntOrString::Int(8080)),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    };
    let _ = svc
        .create(&PostParams::default(), &obj)
        .await
        .or_else(|e| {
            if let kube::Error::Api(ae) = &e {
                if ae.code == 409 {
                    return Ok(obj.clone());
                }
            }
            Err(e)
        })
        .expect("create/get service");
    K8sCleanupGuard::<Service>::new(ns, name, client)
}

pub async fn ensure_deployment_exists(
    client: Client,
    ns: &str,
    name: &str,
) -> K8sCleanupGuard<Deployment> {
    let api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let labels = default_labels(name);
    let dep = Deployment {
        metadata: kube::core::ObjectMeta {
            name: Some(name.to_string()),
            labels: Some(labels.clone()),
            ..Default::default()
        },
        spec: Some(DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector {
                match_labels: Some(labels.clone()),
                ..Default::default()
            },
            template: k8s_openapi::api::core::v1::PodTemplateSpec {
                metadata: Some(kube::core::ObjectMeta {
                    labels: Some(labels.clone()),
                    ..Default::default()
                }),
                spec: Some(k8s_openapi::api::core::v1::PodSpec {
                    containers: vec![k8s_openapi::api::core::v1::Container {
                        name: "main".into(),
                        image: Some("nginx:alpine".into()),
                        ports: Some(vec![
                            k8s_openapi::api::core::v1::ContainerPort {
                                container_port: 8080,
                                ..Default::default()
                            },
                        ]),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    let _ = api
        .create(&PostParams::default(), &dep)
        .await
        .or_else(|e| {
            if let kube::Error::Api(ae) = &e {
                if ae.code == 409 {
                    return Ok(dep.clone());
                }
            }
            Err(e)
        })
        .expect("create/get deployment");
    K8sCleanupGuard::<Deployment>::new(ns, name, client)
}

pub struct K8sCleanupGuard<T>
where
    T: k8s_openapi::Resource<Scope = k8s_openapi::NamespaceResourceScope>
        + k8s_openapi::Metadata<Ty = kube::core::ObjectMeta>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + 'static,
{
    ns: String,
    name: String,
    client: Client,
    _marker: std::marker::PhantomData<T>,
}
impl<T> K8sCleanupGuard<T>
where
    T: k8s_openapi::Resource<Scope = k8s_openapi::NamespaceResourceScope>
        + k8s_openapi::Metadata<Ty = kube::core::ObjectMeta>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + 'static,
{
    pub fn new(ns: &str, name: &str, client: Client) -> Self {
        Self {
            ns: ns.to_string(),
            name: name.to_string(),
            client,
            _marker: std::marker::PhantomData,
        }
    }
}
impl<T> Drop for K8sCleanupGuard<T>
where
    T: k8s_openapi::Resource<Scope = k8s_openapi::NamespaceResourceScope>
        + k8s_openapi::Metadata<Ty = kube::core::ObjectMeta>
        + Clone
        + serde::de::DeserializeOwned
        + std::fmt::Debug
        + 'static,
{
    fn drop(&mut self) {
        let ns = self.ns.clone();
        let name = self.name.clone();
        let client = self.client.clone();
        let _ = tokio::spawn(async move {
            let api: Api<T> = Api::namespaced(client, &ns);
            let _ = api.delete(&name, &Default::default()).await;
        });
    }
}
