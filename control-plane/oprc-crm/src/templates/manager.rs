use super::{DevTemplate, EdgeTemplate, FullTemplate, KnativeTemplate};
use crate::crd::deployment_record::DeploymentRecordSpec;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, PodSpec, PodTemplateSpec, Service, ServicePort,
    ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    LabelSelector, ObjectMeta, OwnerReference,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

#[derive(Debug)]
pub struct TemplateManager {
    templates: Vec<Box<dyn Template + Send + Sync>>,
}

#[derive(Clone, Debug)]
pub enum RenderedResource {
    Deployment(Deployment),
    Service(Service),
    // Future: Knative, CRDs, etc.
    Other {
        api_version: String,
        kind: String,
        manifest: serde_json::Value,
    },
}

pub trait Template: std::fmt::Debug {
    fn name(&self) -> &'static str;
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }
    fn render(&self, ctx: &RenderContext<'_>) -> Vec<RenderedResource>;
    /// Score how suitable this template is for the given environment + NFRs.
    /// Higher is better. Keep small and intuitive.
    fn score(
        &self,
        _env: &EnvironmentContext<'_>,
        _nfr: Option<&crate::crd::deployment_record::NfrRequirementsSpec>,
    ) -> i32 {
        0
    }
}

#[derive(Clone, Debug)]
pub struct RenderContext<'a> {
    pub name: &'a str,
    pub owner_api_version: &'a str,
    pub owner_kind: &'a str,
    pub owner_uid: Option<&'a str>,
    pub enable_odgm_sidecar: bool,
    pub profile: &'a str,
    // Full CRD spec for selection and rendering
    pub spec: &'a DeploymentRecordSpec,
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentContext<'a> {
    pub profile: &'a str,
    pub region: Option<&'a str>,
    pub hardware_class: Option<&'a str>,
}

impl TemplateManager {
    pub fn new(include_knative: bool) -> Self {
        // Minimal built-ins
        let mut templates: Vec<Box<dyn Template + Send + Sync>> = vec![
            Box::new(DevTemplate::default()),
            Box::new(EdgeTemplate::default()),
            Box::new(FullTemplate::default()),
        ];
        if include_knative {
            templates.push(Box::new(KnativeTemplate::default()));
        }
        Self { templates }
    }

    fn select_template<'a>(
        &'a self,
        profile: &str,
        spec: &DeploymentRecordSpec,
    ) -> &'a (dyn Template + Send + Sync) {
        // 1) Explicit hint always wins
        if let Some(h) = spec.selected_template.as_deref() {
            if let Some(t) = self.templates.iter().find(|t| {
                t.name().eq_ignore_ascii_case(h)
                    || t.aliases().iter().any(|a| a.eq_ignore_ascii_case(h))
            }) {
                return t.as_ref();
            }
        }
        // 2) Score by environment + NFRs and pick best (templates own env weighting).
        let envctx = EnvironmentContext {
            profile,
            region: None,
            hardware_class: None,
        };
        let nfr_opt = spec.nfr_requirements.as_ref();
        let mut best: Option<(&Box<dyn Template + Send + Sync>, i32)> = None;
        for t in &self.templates {
            let sc = t.score(&envctx, nfr_opt);
            best = match best {
                Some((_, bs)) if sc > bs => Some((t, sc)),
                Some((bt, bs)) if sc == bs && t.name() < bt.name() => {
                    Some((t, sc))
                }
                Some(prev) => Some(prev),
                None => Some((t, sc)),
            };
        }
        best.map(|(t, _)| t.as_ref())
            .unwrap_or_else(|| self.templates[0].as_ref())
    }

    pub fn render_workload(
        &self,
        ctx: RenderContext<'_>,
    ) -> Vec<RenderedResource> {
        let chosen = self.select_template(ctx.profile, ctx.spec);
        chosen.render(&ctx)
    }
}

pub(crate) fn render_with(
    ctx: &RenderContext<'_>,
    fn_repl: i32,
    odgm_repl: i32,
    odgm_image_override: Option<&str>,
    odgm_port_override: Option<i32>,
) -> Vec<RenderedResource> {
    // Function runtime resources
    let mut fn_lbls = std::collections::BTreeMap::new();
    fn_lbls.insert("app".to_string(), ctx.name.to_string());
    fn_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
    let fn_labels = Some(fn_lbls.clone());
    let fn_selector = LabelSelector {
        match_labels: fn_labels.clone(),
        ..Default::default()
    };

    let func_img = ctx
        .spec
        .function
        .as_ref()
        .and_then(|f| f.image.as_deref())
        .unwrap_or("ghcr.io/pawissanutt/oprc-function:latest");
    let func_port = ctx
        .spec
        .function
        .as_ref()
        .and_then(|f| f.port)
        .unwrap_or(8080);

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
            let odgm_port = odgm_port_override.unwrap_or(8081);
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
            if let Some(cols) = ctx
                .spec
                .odgm_config
                .as_ref()
                .and_then(|o| o.collections.as_ref())
            {
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
            replicas: Some(fn_repl),
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

    let mut resources: Vec<RenderedResource> = vec![
        RenderedResource::Deployment(fn_deployment),
        RenderedResource::Service(fn_service),
    ];

    // ODGM as separate Deployment/Service
    if ctx.enable_odgm_sidecar {
        let odgm_name = format!("{}-odgm", ctx.name);
        let mut odgm_lbls = std::collections::BTreeMap::new();
        odgm_lbls.insert("app".to_string(), odgm_name.clone());
        odgm_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
        let odgm_labels = Some(odgm_lbls.clone());
        let odgm_selector = LabelSelector {
            match_labels: odgm_labels.clone(),
            ..Default::default()
        };
        let odgm_img = odgm_image_override
            .unwrap_or("ghcr.io/pawissanutt/oprc-odgm:latest");
        let odgm_port = odgm_port_override.unwrap_or(8081);

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
        if let Some(cols) = ctx
            .spec
            .odgm_config
            .as_ref()
            .and_then(|o| o.collections.as_ref())
        {
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
                replicas: Some(odgm_repl),
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

        resources.push(RenderedResource::Deployment(odgm_deployment));
        resources.push(RenderedResource::Service(odgm_service));
    }

    resources
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
