use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, PodSpec, PodTemplateSpec, Service, ServicePort,
    ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    LabelSelector, ObjectMeta, OwnerReference,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

#[derive(Clone, Debug, Default)]
pub struct TemplateManager;

#[derive(Clone, Debug)]
pub struct RenderContext<'a> {
    pub name: &'a str,
    pub owner_api_version: &'a str,
    pub owner_kind: &'a str,
    pub owner_uid: Option<&'a str>,
    pub enable_odgm_sidecar: bool,
    pub function_image: Option<&'a str>,
    pub function_port: Option<i32>,
    pub odgm_image: Option<&'a str>,
    pub odgm_port: Option<i32>,
    pub odgm_collections: Option<Vec<String>>,
}

impl TemplateManager {
    pub fn new() -> Self {
        Self
    }

    pub fn render_workload(
        &self,
        ctx: RenderContext<'_>,
    ) -> (Vec<Deployment>, Vec<Service>) {
        // Function runtime resources
        let fn_labels = Some(
            std::iter::once(("app".to_string(), ctx.name.to_string()))
                .collect(),
        );
        let fn_selector = LabelSelector {
            match_labels: fn_labels.clone(),
            ..Default::default()
        };

        let func_img = ctx
            .function_image
            .unwrap_or("ghcr.io/pawissanutt/oprc-function:latest");
        let func_port = ctx.function_port.unwrap_or(8080);

        let mut containers: Vec<Container> = vec![Container {
            name: "function".to_string(),
            image: Some(func_img.to_string()),
            ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                container_port: func_port,
                ..Default::default()
            }]),
            ..Default::default()
        }];

        // If ODGM addon is enabled, inject discovery/env into function container
        if ctx.enable_odgm_sidecar {
            if let Some(func) = containers.first_mut() {
                let odgm_name = format!("{}-odgm", ctx.name);
                let odgm_port = ctx.odgm_port.unwrap_or(8081);
                let odgm_service = format!("{}-svc:{}", odgm_name, odgm_port);
                let mut env = func.env.take().unwrap_or_default();
                env.push(EnvVar {
                    name: "ODGM_ENABLED".to_string(),
                    value: Some("true".to_string()),
                    ..Default::default()
                });
                env.push(EnvVar {
                    name: "ODGM_SERVICE".to_string(),
                    value: Some(odgm_service),
                    ..Default::default()
                });
                if let Some(cols) = &ctx.odgm_collections {
                    if !cols.is_empty() {
                        env.push(EnvVar {
                            name: "ODGM_COLLECTIONS".to_string(),
                            value: Some(cols.join(",")),
                            ..Default::default()
                        });
                    }
                }
                func.env = Some(env);
            }
        }

        let owner_refs = owner_ref(
            ctx.owner_uid,
            ctx.name,
            ctx.owner_api_version,
            ctx.owner_kind,
        );

        let fn_deployment = Deployment {
            metadata: ObjectMeta {
                name: Some(ctx.name.to_string()),
                labels: fn_labels.clone(),
                owner_references: owner_refs.clone(),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                replicas: Some(1),
                selector: fn_selector,
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: fn_labels.clone(),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        containers,
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let fn_svc_name = format!("{}-svc", ctx.name);
        let fn_service = Service {
            metadata: ObjectMeta {
                name: Some(fn_svc_name),
                labels: fn_labels.clone(),
                owner_references: owner_refs,
                ..Default::default()
            },
            spec: Some(ServiceSpec {
                selector: fn_labels,
                ports: Some(vec![ServicePort {
                    port: 80,
                    target_port: Some(IntOrString::Int(func_port)),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };

        let mut deployments = vec![fn_deployment];
        let mut services = vec![fn_service];

        // ODGM as separate Deployment/Service
        if ctx.enable_odgm_sidecar {
            let odgm_name = format!("{}-odgm", ctx.name);
            let odgm_labels = Some(
                std::iter::once(("app".to_string(), odgm_name.clone()))
                    .collect(),
            );
            let odgm_selector = LabelSelector {
                match_labels: odgm_labels.clone(),
                ..Default::default()
            };
            let odgm_img = ctx
                .odgm_image
                .unwrap_or("ghcr.io/pawissanutt/oprc-odgm:latest");
            let odgm_port = ctx.odgm_port.unwrap_or(8081);

            let mut odgm_container = Container {
                name: "odgm".to_string(),
                image: Some(odgm_img.to_string()),
                ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                    container_port: odgm_port,
                    ..Default::default()
                }]),
                env: Some(vec![
                    EnvVar {
                        name: "ODGM_CLUSTER_ID".to_string(),
                        value: Some(ctx.name.to_string()),
                        ..Default::default()
                    },
                    EnvVar {
                        name: "ODGM_SHARDS".to_string(),
                        value: Some("1".to_string()),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            };
            if let Some(cols) = &ctx.odgm_collections {
                if !cols.is_empty() {
                    let mut env = odgm_container.env.take().unwrap_or_default();
                    env.push(EnvVar {
                        name: "ODGM_COLLECTIONS".to_string(),
                        value: Some(cols.join(",")),
                        ..Default::default()
                    });
                    odgm_container.env = Some(env);
                }
            }

            let odgm_owner_refs = owner_ref(
                ctx.owner_uid,
                ctx.name,
                ctx.owner_api_version,
                ctx.owner_kind,
            );
            let odgm_deployment = Deployment {
                metadata: ObjectMeta {
                    name: Some(odgm_name.clone()),
                    labels: odgm_labels.clone(),
                    owner_references: odgm_owner_refs.clone(),
                    ..Default::default()
                },
                spec: Some(DeploymentSpec {
                    replicas: Some(1),
                    selector: odgm_selector,
                    template: PodTemplateSpec {
                        metadata: Some(ObjectMeta {
                            labels: odgm_labels.clone(),
                            ..Default::default()
                        }),
                        spec: Some(PodSpec {
                            containers: vec![odgm_container],
                            ..Default::default()
                        }),
                    },
                    ..Default::default()
                }),
                ..Default::default()
            };

            let odgm_svc_name = format!("{}-svc", odgm_name);
            let odgm_service = Service {
                metadata: ObjectMeta {
                    name: Some(odgm_svc_name),
                    labels: odgm_labels.clone(),
                    owner_references: odgm_owner_refs,
                    ..Default::default()
                },
                spec: Some(ServiceSpec {
                    selector: odgm_labels,
                    ports: Some(vec![ServicePort {
                        port: 80,
                        target_port: Some(IntOrString::Int(odgm_port)),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            };

            deployments.push(odgm_deployment);
            services.push(odgm_service);
        }

        (deployments, services)
    }
}

fn owner_ref(
    uid: Option<&str>,
    name: &str,
    api_version: &str,
    kind: &str,
) -> Option<Vec<OwnerReference>> {
    uid.map(|u| {
        vec![OwnerReference {
            api_version: api_version.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            uid: u.to_string(),
            controller: Some(true),
            block_owner_deletion: None,
        }]
    })
}
