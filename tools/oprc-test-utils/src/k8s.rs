use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{Service, ServicePort};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{
    Client,
    api::{Api, ListParams, PostParams},
};
use std::collections::BTreeMap;
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
        let _ = tokio::spawn(async move {
            cleanup_k8s(&ns, &name, client, include_hpa).await;
        });
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
