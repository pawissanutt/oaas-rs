use super::{
    DevTemplate, EdgeTemplate, K8sDeploymentTemplate, KnativeTemplate,
};
use crate::crd::class_runtime::ClassRuntimeSpec;
use crate::crd::class_runtime::FunctionRoute; // reuse CRD route type for predicted routes
use crate::templates::odgm; // shared ODGM helpers now also provide env + resource builders
use envconfig::Envconfig;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
    Container, EnvVar, PodSpec, PodTemplateSpec, Service, ServicePort,
    ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
    LabelSelector, ObjectMeta, OwnerReference,
};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use serde_json; // for helper JSON env constructors
use std::collections::BTreeMap;

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
    fn render(
        &self,
        ctx: &RenderContext<'_>,
    ) -> Result<Vec<RenderedResource>, TemplateError>;
    /// Score how suitable this template is for the given environment + NFRs.
    /// Higher is better. Keep small and intuitive.
    fn score(
        &self,
        _env: &EnvironmentContext<'_>,
        _nfr: Option<&crate::crd::class_runtime::NfrRequirementsSpec>,
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
    /// Optional in-namespace Zenoh router Service name and port discovered by the controller.
    /// When present, templates should wire OPRC_ZENOH_* envs accordingly.
    pub router_service_name: Option<String>,
    pub router_service_port: Option<i32>,
    // Full CRD spec for selection and rendering
    pub spec: &'a ClassRuntimeSpec,
}

#[derive(Clone, Copy, Debug)]
pub struct EnvironmentContext<'a> {
    pub profile: &'a str,
    pub region: Option<&'a str>,
    pub hardware_class: Option<&'a str>,
    /// Optional deployment zone/availability-zone hint
    pub zone: Option<&'a str>,
    pub is_datacenter: bool,
    pub is_edge: bool,
}

/// Owned configuration that can be populated from environment variables using
/// the `envconfig` crate and then converted into an `EnvironmentContext<'static>`.
#[derive(Envconfig, Clone, Debug)]
pub struct EnvCtxConfig {
    /// Profile name (dev|edge|full). Env: OPRC_CRM_PROFILE
    #[envconfig(from = "OPRC_CRM_PROFILE", default = "dev")]
    pub profile: String,

    /// Deployment region (optional). Env: OPRC_ENV_REGION
    #[envconfig(from = "OPRC_ENV_REGION")]
    pub region: Option<String>,

    /// Hardware class (optional). Env: OPRC_ENV_HW_CLASS
    #[envconfig(from = "OPRC_ENV_HW_CLASS")]
    pub hardware_class: Option<String>,

    /// Is this running in a datacenter environment? Env: OPRC_ENV_IS_DATACENTER
    #[envconfig(from = "OPRC_ENV_IS_DATACENTER", default = "false")]
    pub is_datacenter: bool,

    /// Is this running on an edge node? Env: OPRC_ENV_IS_EDGE
    #[envconfig(from = "OPRC_ENV_IS_EDGE", default = "false")]
    pub is_edge: bool,

    /// Deployment zone / availability zone (optional). Env: OPRC_ENV_ZONE
    #[envconfig(from = "OPRC_ENV_ZONE")]
    pub zone: Option<String>,
}

/// Owned Environment context produced from environment variables. This is
/// returned by `env_from_env` and can be converted into a borrowed
/// `EnvironmentContext<'_>` for passing into template scoring without leaking.
#[derive(Clone, Debug)]
pub struct EnvironmentOwned {
    pub profile: String,
    pub region: Option<String>,
    pub hardware_class: Option<String>,
    pub zone: Option<String>,
    pub is_datacenter: bool,
    pub is_edge: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum TemplateError {
    #[error("function image must be provided in ClassRuntime spec")]
    MissingFunctionImage,
    #[error("failed to build ODGM collections JSON: {0}")]
    OdgmCollectionsJson(#[from] serde_json::Error),
}

impl TemplateManager {
    pub fn new(include_knative: bool) -> Self {
        // Minimal built-ins
        let mut templates: Vec<Box<dyn Template + Send + Sync>> = vec![
            Box::new(DevTemplate::default()),
            Box::new(EdgeTemplate::default()),
            Box::new(K8sDeploymentTemplate::default()),
        ];
        if include_knative {
            templates.push(Box::new(KnativeTemplate::default()));
        }
        Self { templates }
    }

    fn select_template<'a>(
        &'a self,
        profile: &str,
        spec: &ClassRuntimeSpec,
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
        // Build an EnvironmentContext from environment variables when possible so
        // template scoring can use runtime topology hints (region, hw class,
        // datacenter/edge flags). If env parsing fails, fall back to the
        // provided profile and conservative defaults.
        let env_owned = TemplateManager::env_from_env(profile);
        // create a borrowed view for scoring without leaking
        let env = EnvironmentContext {
            profile: &env_owned.profile,
            region: env_owned.region.as_deref(),
            hardware_class: env_owned.hardware_class.as_deref(),
            zone: env_owned.zone.as_deref(),
            is_datacenter: env_owned.is_datacenter,
            is_edge: env_owned.is_edge,
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

    /// Read environment variables into an owned `EnvCtxConfig` and convert to
    /// an `EnvironmentContext<'static>`. This uses `envconfig` and will
    /// default missing values; any parse errors fall back to conservative
    /// defaults matching the provided profile.
    pub fn env_from_env(profile_override: &str) -> EnvironmentOwned {
        // Attempt to load config from environment; if it fails, fall back to
        // defaults created from the override profile.
        match EnvCtxConfig::init_from_env() {
            Ok(mut cfg) => {
                // Respect caller-provided profile override (e.g. tests or runtime
                // caller) as authoritative. This avoids envconfig defaults
                // (which default to "dev") masking the desired profile passed
                // by the caller.
                if !profile_override.is_empty() {
                    cfg.profile = profile_override.to_string();
                }
                EnvironmentOwned {
                    profile: cfg.profile,
                    region: cfg.region,
                    hardware_class: cfg.hardware_class,
                    zone: cfg.zone,
                    is_datacenter: cfg.is_datacenter,
                    is_edge: cfg.is_edge,
                }
            }
            Err(_) => EnvironmentOwned {
                profile: profile_override.to_string(),
                region: None,
                hardware_class: None,
                zone: None,
                is_datacenter: false,
                is_edge: false,
            },
        }
    }

    pub fn render_workload(
        &self,
        ctx: RenderContext<'_>,
    ) -> Result<Vec<RenderedResource>, TemplateError> {
        // Validate each function has an image available in provision_config
        for f in ctx.spec.functions.iter() {
            let has_img = f
                .provision_config
                .as_ref()
                .and_then(|p| p.container_image.as_ref())
                .is_some();
            if !has_img {
                return Err(TemplateError::MissingFunctionImage);
            }
        }

        let tpl = self.select_template(ctx.profile, ctx.spec);
        let resources = tpl.render(&ctx)?;
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
        if s.is_empty() {
            "default".to_string()
        } else {
            s
        }
    }

    /// Render k8s Deployment/Service resources for functions and optional ODGM
    pub fn render_with(
        ctx: &RenderContext<'_>,
        odgm_repl: i32,
        odgm_image_override: Option<&str>,
    ) -> Result<Vec<RenderedResource>, TemplateError> {
        let mut resources: Vec<RenderedResource> = Vec::new();

        for (i, f) in ctx.spec.functions.iter().enumerate() {
            let fn_name = crate::routing::function_service_name(
                ctx.name,
                i,
                ctx.spec.functions.len(),
            );

            let mut fn_lbls = std::collections::BTreeMap::new();
            fn_lbls.insert("app".to_string(), fn_name.clone());
            fn_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
            let fn_labels = Some(fn_lbls.clone());
            let fn_selector = LabelSelector {
                match_labels: fn_labels.clone(),
                ..Default::default()
            };

            let func_img = f
                .provision_config
                .as_ref()
                .and_then(|p| p.container_image.as_deref())
                .ok_or(TemplateError::MissingFunctionImage)?;
            let func_port = f
                .provision_config
                .as_ref()
                .and_then(|p| p.port)
                .unwrap_or(80u16) as i32;

            let mut containers: Vec<Container> = vec![Container {
                name: "function".to_string(),
                image: Some(func_img.to_string()),
                ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                    container_port: func_port,
                    ..Default::default()
                }]),
                ..Default::default()
            }];

            // Inject function config and optional ODGM env into the container env
            if let Some(container) = containers.first_mut() {
                let mut env: Vec<EnvVar> =
                    container.env.take().unwrap_or_default();
                // Map function config key-values to env vars
                for (k, v) in &f.config {
                    env.push(EnvVar {
                        name: k.clone(),
                        value: Some(v.clone()),
                        ..Default::default()
                    });
                }
                // Append ODGM env when enabled
                if ctx.enable_odgm_sidecar {
                    let addl = odgm::build_function_odgm_env_k8s(ctx)?;
                    env.extend(addl);
                }
                if !env.is_empty() {
                    container.env = Some(env);
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
                    replicas: Some(
                        f.provision_config
                            .as_ref()
                            .and_then(|p| p.min_scale)
                            .unwrap_or(1) as i32,
                    ),
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
                    name: Some(fn_name.clone()),
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
            let (odgm_deployment, odgm_service) = odgm::build_odgm_resources(
                ctx,
                odgm_repl,
                odgm_image_override,
                true, // include owner refs in k8s deploy template path
            )?;
            resources.push(RenderedResource::Deployment(odgm_deployment));
            resources.push(RenderedResource::Service(odgm_service));
        }

        Ok(resources)
    }

    /// Build predicted (deterministic) in-cluster HTTP URLs for each function and
    /// return a map usable as fn_routes in an InvocationsSpec. Keys are the
    /// binding names (method names) when available; values include function_key for mapping.
    pub fn predicted_function_routes(
        ctx: &RenderContext<'_>,
    ) -> BTreeMap<String, FunctionRoute> {
        let mut routes = BTreeMap::new();
        if ctx.spec.functions.is_empty() {
            tracing::trace!(name=%ctx.name, "predicted_function_routes: no functions");
            return routes;
        }
        let multi = ctx.spec.functions.len() > 1;
        tracing::debug!(name=%ctx.name, count=ctx.spec.functions.len(), multi=%multi, "predicted_function_routes: building predicted routes");
        for (i, f) in ctx.spec.functions.iter().enumerate() {
            let url = crate::routing::function_service_url(
                ctx.name,
                i,
                ctx.spec.functions.len(),
            );
            // Use the function_key suffix after last dot as a default method name,
            // but prefer matching user-provided binding names during merge; here we predict
            // using the suffix so it aligns with common binding names when PM is absent.
            let default_method = f
                .function_key
                .rsplit('.')
                .next()
                .unwrap_or(&f.function_key)
                .to_string();
            routes.entry(default_method).or_insert(FunctionRoute {
                url,
                stateless: Some(true),
                standby: None,
                active_group: Vec::new(),
                function_key: Some(f.function_key.clone()),
            });
        }
        tracing::trace!(name=%ctx.name, keys=?routes.keys().collect::<Vec<_>>(), "predicted_function_routes: completed");
        routes
    }
}

// Module-level wrappers so other templates can import these helpers directly
pub fn dns1035_safe(name: &str) -> String {
    TemplateManager::dns1035_safe(name)
}

pub fn render_with(
    ctx: &RenderContext<'_>,
    odgm_repl: i32,
    odgm_image_override: Option<&str>,
) -> Result<Vec<RenderedResource>, TemplateError> {
    TemplateManager::render_with(ctx, odgm_repl, odgm_image_override)
}

// owner_ref moved to odgm helpers; keep this re-export transitional if needed
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
    use crate::crd::class_runtime::{
        ClassRuntimeSpec as DeploymentRecordSpec, FunctionRoute, FunctionSpec,
        InvocationsSpec, OdgmConfigSpec,
    };
    use oprc_models::ProvisionConfig;

    fn base_spec() -> DeploymentRecordSpec {
        DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            functions: vec![FunctionSpec {
                function_key: "fn-1".into(),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: Some(ProvisionConfig {
                    container_image: Some("img:function".into()),
                    ..Default::default()
                }),
                config: std::collections::HashMap::new(),
            }],
            nfr_requirements: None,
            ..Default::default()
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
            ..Default::default()
        });
        let ctx = RenderContext {
            name: "class-z",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "full",
            router_service_name: None,
            router_service_port: None,
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

    #[test]
    fn template_manager_errors_when_function_image_missing() {
        let spec = DeploymentRecordSpec {
            functions: vec![FunctionSpec {
                function_key: "fn-x".into(),
                description: None,
                available_location: None,
                qos_requirement: None,
                provision_config: None,
                config: std::collections::HashMap::new(),
            }],
            ..Default::default()
        };
        let ctx = RenderContext {
            name: "class-missing",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: false,
            profile: "dev",
            router_service_name: None,
            router_service_port: None,
            spec: &spec,
        };
        let tm = TemplateManager::new(false);
        let err = tm
            .render_workload(ctx)
            .expect_err("expected missing image error");
        matches!(err, TemplateError::MissingFunctionImage);
    }

    #[test]
    fn template_error_from_serde_json_maps_to_odgm_variant() {
        // Create a serde_json::Error by attempting to parse invalid JSON
        let sj_err =
            serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let te: TemplateError = sj_err.into();
        assert!(matches!(te, TemplateError::OdgmCollectionsJson(_)));
    }

    // --- New tests for predicted ODGM invocation routes ---

    #[test]
    fn odgm_invocations_auto_populates_single_route() {
        let mut spec = base_spec();
        // Ensure container port is custom to verify URL includes it
        spec.functions[0].provision_config.as_mut().unwrap().port = Some(9090);
        spec.odgm_config = Some(OdgmConfigSpec {
            collections: Some(vec!["orders".into()]),
            partition_count: Some(1),
            replica_count: Some(1),
            shard_type: Some("mst".into()),
            ..Default::default()
        });
        let ctx = RenderContext {
            name: "OrderSvc", // mixed case to test dns lowering
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "dev",
            router_service_name: None,
            router_service_port: None,
            spec: &spec,
        };
        let tm = TemplateManager::new(false);
        let resources = tm.render_workload(ctx).expect("render workload");
        // Find function deployment env vars
        let fn_dep = resources
            .iter()
            .find_map(|r| match r {
                RenderedResource::Deployment(d)
                    if d.metadata.name.as_ref().unwrap() == "ordersvc" =>
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
            serde_json::from_str(col_env.value.as_ref().unwrap()).unwrap();
        let first = &parsed.as_array().unwrap()[0];
        // invocations -> fn_routes -> fn-1
        let fn_routes = first
            .get("invocations")
            .and_then(|v| v.get("fn_routes"))
            .and_then(|v| v.as_object())
            .expect("fn_routes present");
        let route = fn_routes.get("fn-1").expect("route for fn-1");
        let url = route.get("url").and_then(|v| v.as_str()).unwrap();
        assert_eq!(url, "http://ordersvc:80/"); // single function Service name, svc maps to 80
        // Defaults
        assert_eq!(route.get("stateless").unwrap().as_bool().unwrap(), true);
        assert_eq!(route.get("standby").unwrap().as_bool().unwrap(), false);
    }

    #[test]
    fn odgm_invocations_merges_user_and_predicted() {
        // Two functions; user supplies route only for first; second should be auto-added
        let mut spec = DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            functions: vec![
                FunctionSpec {
                    function_key: "fn-a".into(),
                    description: None,
                    available_location: None,
                    qos_requirement: None,
                    provision_config: Some(ProvisionConfig {
                        container_image: Some("img:a".into()),
                        port: Some(9000),
                        ..Default::default()
                    }),
                    config: std::collections::HashMap::new(),
                },
                FunctionSpec {
                    function_key: "fn-b".into(),
                    description: None,
                    available_location: None,
                    qos_requirement: None,
                    provision_config: Some(ProvisionConfig {
                        container_image: Some("img:b".into()),
                        port: None, // default fallback 8080
                        ..Default::default()
                    }),
                    config: std::collections::HashMap::new(),
                },
            ],
            nfr_requirements: None,
            ..Default::default()
        };
        // User-provided partial invocations (only fn-a)
        let mut user_routes = std::collections::BTreeMap::new();
        user_routes.insert(
            "fn-a".to_string(),
            FunctionRoute {
                url: "http://custom-a:9000/".into(),
                stateless: Some(false), // user overrides default
                standby: Some(true),
                active_group: vec![1, 2],
                function_key: Some("fn-a".into()),
            },
        );
        spec.odgm_config = Some(OdgmConfigSpec {
            collections: Some(vec!["c1".into()]),
            partition_count: Some(1),
            replica_count: Some(1),
            shard_type: Some("mst".into()),
            invocations: Some(InvocationsSpec {
                fn_routes: user_routes,
                disabled_fn: vec![],
            }),
            ..Default::default()
        });
        let ctx = RenderContext {
            name: "OrderSvc",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "dev",
            router_service_name: None,
            router_service_port: None,
            spec: &spec,
        };
        let tm = TemplateManager::new(false);
        let resources = tm.render_workload(ctx).expect("render workload");
        let fn_dep = resources
            .iter()
            .find_map(|r| match r {
                RenderedResource::Deployment(d)
                    if d.metadata.name.as_ref().unwrap() == "ordersvc-fn-0" =>
                {
                    Some(d)
                }
                _ => None,
            })
            .expect("first function deployment");
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
            serde_json::from_str(col_env.value.as_ref().unwrap()).unwrap();
        let first = &parsed.as_array().unwrap()[0];
        let fn_routes = first
            .get("invocations")
            .and_then(|v| v.get("fn_routes"))
            .and_then(|v| v.as_object())
            .expect("fn_routes present");
        // User-supplied route preserved
        let route_a = fn_routes.get("fn-a").expect("fn-a present");
        assert_eq!(
            route_a.get("url").and_then(|v| v.as_str()).unwrap(),
            "http://custom-a:9000/"
        );
        assert_eq!(route_a.get("stateless").unwrap().as_bool().unwrap(), false);
        assert_eq!(route_a.get("standby").unwrap().as_bool().unwrap(), true);
        // Predicted route added for fn-b
        let route_b = fn_routes.get("fn-b").expect("fn-b present");
        assert_eq!(
            route_b.get("url").and_then(|v| v.as_str()).unwrap(),
            // second function index 1 -> service ordersvc-fn-1 default port 8080
            "http://ordersvc-fn-1:80/"
        );
        // Defaults applied
        assert_eq!(route_b.get("stateless").unwrap().as_bool().unwrap(), true);
        assert_eq!(route_b.get("standby").unwrap().as_bool().unwrap(), false);
        // No function_key included in ODGM_COLLECTION JSON (protobuf route)
    }

    #[test]
    fn odgm_invocations_multi_function_all_default_ports() {
        // Two functions with no user routes; both should appear with default 8080 ports
        let spec = DeploymentRecordSpec {
            functions: vec![
                FunctionSpec {
                    function_key: "create".into(),
                    description: None,
                    available_location: None,
                    qos_requirement: None,
                    provision_config: Some(ProvisionConfig {
                        container_image: Some("img:a".into()),
                        ..Default::default()
                    }),
                    config: std::collections::HashMap::new(),
                },
                FunctionSpec {
                    function_key: "read".into(),
                    description: None,
                    available_location: None,
                    qos_requirement: None,
                    provision_config: Some(ProvisionConfig {
                        container_image: Some("img:b".into()),
                        ..Default::default()
                    }),
                    config: std::collections::HashMap::new(),
                },
            ],
            odgm_config: Some(OdgmConfigSpec {
                collections: Some(vec!["c1".into()]),
                partition_count: Some(1),
                replica_count: Some(1),
                shard_type: Some("mst".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let ctx = RenderContext {
            name: "OrderSvc",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "dev",
            router_service_name: None,
            router_service_port: None,
            spec: &spec,
        };
        let tm = TemplateManager::new(false);
        let resources = tm.render_workload(ctx).unwrap();
        let fn_dep = resources
            .iter()
            .find_map(|r| match r {
                RenderedResource::Deployment(d)
                    if d.metadata.name.as_ref().unwrap() == "ordersvc-fn-0" =>
                {
                    Some(d)
                }
                _ => None,
            })
            .unwrap();
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
            .unwrap();
        let parsed: serde_json::Value =
            serde_json::from_str(col_env.value.as_ref().unwrap()).unwrap();
        let first = &parsed.as_array().unwrap()[0];
        let fn_routes = first
            .get("invocations")
            .and_then(|v| v.get("fn_routes"))
            .and_then(|v| v.as_object())
            .unwrap();
        assert_eq!(fn_routes.len(), 2, "expected two predicted routes");
        let create_url = fn_routes
            .get("create")
            .unwrap()
            .get("url")
            .unwrap()
            .as_str()
            .unwrap();
        let read_url = fn_routes
            .get("read")
            .unwrap()
            .get("url")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(create_url, "http://ordersvc-fn-0:80/");
        assert_eq!(read_url, "http://ordersvc-fn-1:80/");
    }

    #[test]
    fn odgm_invocations_not_injected_when_sidecar_disabled() {
        let mut spec = base_spec();
        spec.odgm_config = Some(OdgmConfigSpec {
            collections: Some(vec!["orders".into()]),
            partition_count: Some(1),
            replica_count: Some(1),
            shard_type: Some("mst".into()),
            ..Default::default()
        });
        let ctx = RenderContext {
            name: "class-z",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: false,
            profile: "dev",
            router_service_name: None,
            router_service_port: None,
            spec: &spec,
        };
        let tm = TemplateManager::new(false);
        let resources = tm.render_workload(ctx).unwrap();
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
            .unwrap();
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
            .as_ref();
        assert!(
            env_vars.is_none()
                || !env_vars
                    .unwrap()
                    .iter()
                    .any(|e| e.name == "ODGM_COLLECTION"),
            "ODGM_COLLECTION should not be injected when sidecar disabled"
        );
    }

    #[test]
    fn odgm_no_functions_yields_no_routes() {
        let spec = DeploymentRecordSpec {
            functions: vec![],
            ..Default::default()
        };
        let ctx = RenderContext {
            name: "empty",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "ClassRuntime",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "dev",
            router_service_name: None,
            router_service_port: None,
            spec: &spec,
        };
        let routes = TemplateManager::predicted_function_routes(&ctx);
        assert!(routes.is_empty());
    }
}
