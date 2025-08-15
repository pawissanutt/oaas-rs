#![allow(dead_code)]

use std::collections::BTreeMap;
use std::time::Duration;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::autoscaling::v2 as autoscalingv2;
use k8s_openapi::api::core::v1::{Service, ServicePort};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{
    Client,
    api::{Api, ListParams, PostParams},
};
use tokio::task::JoinHandle;

// DNS-1123 safe numeric suffix for unique names
pub const DIGITS: [char; 10] =
    ['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];
pub fn uniq(prefix: &str) -> String {
    format!("{prefix}-{}", nanoid::nanoid!(6, &DIGITS))
}

// Env guard utilities
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
    let dep_api: Api<Deployment> = Api::namespaced(client, ns);
    for _ in 0..60 {
        if dep_api.get_opt(name).await.unwrap_or(None).is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    panic!("deployment {}/{} not found in time", ns, name);
}

pub async fn cleanup_k8s(
    ns: &str,
    name: &str,
    client: Client,
    include_hpa: bool,
) {
    // Best-effort cleanup of children and DR/HPA
    use oprc_crm::crd::deployment_record::DeploymentRecord;
    let dr_api: Api<DeploymentRecord> = Api::namespaced(client.clone(), ns);
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let lp = ListParams::default().labels(&format!("oaas.io/owner={}", name));

    if let Ok(list) = dep_api.list(&lp).await {
        for d in list {
            let _ = dep_api
                .delete(
                    &d.metadata.name.unwrap_or_else(|| name.to_string()),
                    &Default::default(),
                )
                .await;
        }
    }
    if let Ok(list) = svc_api.list(&lp).await {
        for s in list {
            let _ = svc_api
                .delete(
                    &s.metadata.name.unwrap_or_else(|| format!("{}-svc", name)),
                    &Default::default(),
                )
                .await;
        }
    }
    if include_hpa {
        let hpa_api: Api<autoscalingv2::HorizontalPodAutoscaler> =
            Api::namespaced(client.clone(), ns);
        let _ = hpa_api.delete(name, &Default::default()).await;
    }
    let _ = dr_api.delete(name, &Default::default()).await;
}

// RAII guard to ensure controller abort + cleanup
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
        if let Some(ref handle) = self.ctrl {
            handle.abort();
        }
        let ns = self.ns.clone();
        let name = self.name.clone();
        let client = self.client.clone();
        let include_hpa = self.include_hpa;
        let _ = tokio::spawn(async move {
            cleanup_k8s(&ns, &name, client, include_hpa).await;
        });
    }
}

// Label helpers and resource creation (for it_k8s)
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
    let svc_api: Api<Service> = Api::namespaced(client.clone(), ns);
    let labels = default_labels(name);
    let svc = Service {
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
    let _ = svc_api
        .create(&PostParams::default(), &svc)
        .await
        .or_else(|e| {
            if let kube::Error::Api(ae) = &e {
                if ae.code == 409 {
                    return Ok(svc.clone());
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
    let dep_api: Api<Deployment> = Api::namespaced(client.clone(), ns);
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
    let _ = dep_api
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

// Minimal cleanup guard to remove created k8s object by name
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
