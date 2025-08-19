use super::{DevTemplate, EdgeTemplate, FullTemplate, KnativeTemplate};
use crate::crd::deployment_record::DeploymentRecordSpec;
use crate::templates::odgm;
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

#[derive(thiserror::Error, Debug)]
pub enum TemplateError {
    #[error("function image must be provided in DeploymentRecord spec")]
    MissingFunctionImage,
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
            for t in &self.templates {
                if t.name() == h || t.aliases().iter().any(|a| *a == h) {
                    return &**t;
                }
            }
        }

        // 2) Pick by score heuristics
        let env = EnvironmentContext {
            profile,
            region: None,
            hardware_class: None,
        };
        let mut best = &*self.templates[0];
        let mut best_score = best.score(&env, spec.nfr_requirements.as_ref());
        for t in &self.templates {
            let s = t.score(&env, spec.nfr_requirements.as_ref());
            if s > best_score {
                best = &**t;
                best_score = s;
            }
        }
        best
    }

    pub fn render_workload(
        &self,
        ctx: RenderContext<'_>,
    ) -> Result<Vec<RenderedResource>, TemplateError> {
        // Validate each function has an image available (either container_image or provision_config)
        for f in ctx.spec.functions.iter() {
            let has_img = f
                .container_image
                .as_ref()
                .or_else(|| {
                    f.provision_config
                        .as_ref()
                        .and_then(|p| p.container_image.as_ref())
                })
                .is_some();
            if !has_img {
                return Err(TemplateError::MissingFunctionImage);
            }
        }

        let tpl = self.select_template(ctx.profile, ctx.spec);
        let resources = tpl.render(&ctx);
        Ok(resources)
    }

    // Helper: make a DNS-1035-safe name
    pub fn dns1035_safe(name: &str) -> String {
        let mut s: String = name
            .to_ascii_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        // Trim leading/trailing hyphens
        while s.starts_with('-') {
            s.remove(0);
        }
        while s.ends_with('-') {
            s.pop();
        }
        if s.is_empty() { "a".to_string() } else { s }
    }

    /// Render k8s Deployment/Service resources for functions and optional ODGM
    pub fn render_with(
        ctx: &RenderContext<'_>,
        _fn_repl: i32,
        _odgm_repl: i32,
        odgm_image_override: Option<&str>,
        odgm_port_override: Option<i32>,
    ) -> Vec<RenderedResource> {
        let mut resources: Vec<RenderedResource> = Vec::new();
        let safe_base = dns1035_safe(ctx.name);

        for (i, f) in ctx.spec.functions.iter().enumerate() {
            let fn_name = if ctx.spec.functions.len() == 1 {
                safe_base.clone()
            } else {
                format!("{}-fn-{}", safe_base, i)
            };

            let mut fn_lbls = std::collections::BTreeMap::new();
            fn_lbls.insert("app".to_string(), fn_name.clone());
            fn_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
            let fn_labels = Some(fn_lbls.clone());
            let fn_selector = LabelSelector {
                match_labels: fn_labels.clone(),
                ..Default::default()
            };

            let func_img = f
                .container_image
                .as_deref()
                .or_else(|| {
                    f.provision_config
                        .as_ref()
                        .and_then(|p| p.container_image.as_deref())
                })
                .unwrap_or("<missing-function-image>");
            let func_port = f
                .provision_config
                .as_ref()
                .and_then(|p| p.port)
                .unwrap_or(8080u16) as i32;

            let mut containers: Vec<Container> = vec![Container {
                name: "function".to_string(),
                image: Some(func_img.to_string()),
                ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                    container_port: func_port,
                    ..Default::default()
                }]),
                ..Default::default()
            }];

            // Inject ODGM env into each function container when enabled
            if ctx.enable_odgm_sidecar {
                if let Some(func) = containers.first_mut() {
                    let odgm_name = format!("{}-odgm", ctx.name);
                    let odgm_port = odgm_port_override.unwrap_or(8081);
                    let odgm_service =
                        format!("{}-svc:{}", odgm_name, odgm_port);
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
                    if let Some(cols) = odgm::collection_names(ctx.spec) {
                        env.push(odgm::collections_env_var(cols, ctx.spec));
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

            let deployment = Deployment {
                metadata: ObjectMeta {
                    name: Some(fn_name.clone()),
                    labels: Some(fn_lbls.clone()),
                    owner_references: owner_refs.clone(),
                    ..Default::default()
                },
                spec: Some(DeploymentSpec {
                    replicas: Some(f.replicas as i32),
                    selector: fn_selector,
                    template: PodTemplateSpec {
                        metadata: Some(ObjectMeta {
                            labels: Some(fn_lbls.clone()),
                            ..Default::default()
                        }),
                        spec: Some(PodSpec {
                            containers: containers.clone(),
                            ..Default::default()
                        }),
                    },
                    ..Default::default()
                }),
                ..Default::default()
            };

            let svc = Service {
                metadata: ObjectMeta {
                    name: Some(format!("{}-svc", fn_name)),
                    labels: Some(fn_lbls.clone()),
                    owner_references: owner_refs.clone(),
                    ..Default::default()
                },
                spec: Some(ServiceSpec {
                    selector: Some(fn_lbls.clone()),
                    ports: Some(vec![ServicePort {
                        port: 80,
                        target_port: Some(IntOrString::Int(func_port)),
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            };

            resources.push(RenderedResource::Deployment(deployment));
            resources.push(RenderedResource::Service(svc));
        }

        // ODGM as separate Deployment/Service
        if ctx.enable_odgm_sidecar {
            let odgm_name = format!("{}-odgm", dns1035_safe(ctx.name));
            let mut odgm_lbls = std::collections::BTreeMap::new();
            odgm_lbls.insert("app".to_string(), odgm_name.clone());
            odgm_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
            let odgm_labels = Some(odgm_lbls.clone());
            let odgm_selector = LabelSelector {
                match_labels: odgm_labels.clone(),
                ..Default::default()
            };
            let odgm_img = odgm_image_override
                .unwrap_or("ghcr.io/pawissanutt/oaas/odgm:latest");
            let odgm_port = odgm_port_override.unwrap_or(8081);

            let mut odgm_container = Container {
                name: "odgm".to_string(),
                image: Some(odgm_img.to_string()),
                ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                    container_port: odgm_port,
                    ..Default::default()
                }]),
                env: Some(vec![EnvVar {
                    name: "ODGM_CLUSTER_ID".to_string(),
                    value: Some(ctx.name.to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            };
            if let Some(cols) = odgm::collection_names(ctx.spec) {
                let mut env = odgm_container.env.take().unwrap_or_default();
                env.push(odgm::collections_env_var(cols, ctx.spec));
                odgm_container.env = Some(env);
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
                    replicas: Some(_odgm_repl),
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
}

// Module-level wrappers so other templates can import these helpers directly
pub fn dns1035_safe(name: &str) -> String {
    TemplateManager::dns1035_safe(name)
}

pub fn render_with(
    ctx: &RenderContext<'_>,
    fn_repl: i32,
    odgm_repl: i32,
    odgm_image_override: Option<&str>,
    odgm_port_override: Option<i32>,
) -> Vec<RenderedResource> {
    TemplateManager::render_with(
        ctx,
        fn_repl,
        odgm_repl,
        odgm_image_override,
        odgm_port_override,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::deployment_record::{
        DeploymentRecordSpec, FunctionSpec, OdgmConfigSpec,
    };

    fn base_spec() -> DeploymentRecordSpec {
        DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            functions: vec![FunctionSpec {
                function_key: "fn-1".into(),
                replicas: 1,
                container_image: Some("img:function".into()),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: None,
                config: std::collections::HashMap::new(),
            }],
            nfr_requirements: None,
            nfr: None,
        }
    }

    #[test]
    fn odgm_collection_env_includes_partition_and_replica() {
        let mut spec = base_spec();
        spec.odgm_config = Some(OdgmConfigSpec {
            collections: Some(vec!["orders".into()]),
            partition_count: Some(3),
            replica_count: Some(2),
            shard_type: Some("mst".into()),
        });
        let ctx = RenderContext {
            name: "class-z",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "DeploymentRecord",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "full",
            spec: &spec,
        };
        let tm = TemplateManager::new(false);
        let resources = tm.render_workload(ctx).expect("render workload");
        let fn_dep = resources
            .iter()
            .find_map(|r| match r {
                RenderedResource::Deployment(d)
                    if d.metadata.name.as_ref().unwrap() == "class-z" =>
                {
                    Some(d)
                }
                _ => None,
            })
            .expect("fn deployment");
        let env_vars = fn_dep
            .spec
            .as_ref()
            .unwrap()
            .template
            .spec
            .as_ref()
            .unwrap()
            .containers[0]
            .env
            .as_ref()
            .unwrap();
        let col_env = env_vars
            .iter()
            .find(|e| e.name == "ODGM_COLLECTION")
            .expect("odgm collection env");
        let parsed: serde_json::Value =
            serde_json::from_str(col_env.value.as_ref().unwrap())
                .expect("json");
        assert!(parsed.is_array());
        let first = &parsed.as_array().unwrap()[0];
        assert_eq!(first.get("partition_count").unwrap().as_i64().unwrap(), 3);
        assert_eq!(first.get("replica_count").unwrap().as_i64().unwrap(), 2);
        assert_eq!(first.get("shard_type").unwrap().as_str().unwrap(), "mst");
    }
}
