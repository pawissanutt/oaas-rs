use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template, dns1035_safe,
};
use crate::templates::odgm;
use crate::templates::odgm::build_function_odgm_env_json;

#[derive(Clone, Debug, Default)]
pub struct KnativeTemplate;

impl Template for KnativeTemplate {
    fn name(&self) -> &'static str {
        "knative"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["kn", "knsvc"]
    }

    fn render(
        &self,
        ctx: &RenderContext<'_>,
    ) -> Result<Vec<RenderedResource>, super::TemplateError> {
        // Render a Knative Service instead of Deployment/Service.
        // We still propagate ODGM env to function via annotations/env when applicable in a future pass.
        // Use the shared model: provision_config.container_image and provision_config.port
        let _fn_img = ctx
            .spec
            .functions
            .first()
            .and_then(|f| {
                f.provision_config
                    .as_ref()
                    .and_then(|p| p.container_image.as_deref())
            })
            .expect(
                "function image validated in TemplateManager::render_workload",
            );
        let _fn_port = ctx
            .spec
            .functions
            .first()
            .and_then(|f| f.provision_config.as_ref().and_then(|p| p.port))
            .unwrap_or(8080);

        // Render one Knative Service per function in the spec.
        let safe_name = dns1035_safe(ctx.name);
        let mut resources: Vec<RenderedResource> = Vec::new();
        for (i, f) in ctx.spec.functions.iter().enumerate() {
            // Build annotations and labels per service
            let mut annotations = std::collections::BTreeMap::new();
            annotations
                .insert("prometheus.io/scrape".to_string(), "true".to_string());
            let fn_port = f
                .provision_config
                .as_ref()
                .and_then(|p| p.port)
                .unwrap_or(8080);
            annotations
                .insert("prometheus.io/port".to_string(), fn_port.to_string());
            annotations.insert(
                "prometheus.io/path".to_string(),
                "/metrics".to_string(),
            );

            let mut labels = std::collections::BTreeMap::new();
            labels.insert("oaas.io/owner".to_string(), safe_name.clone());
            let svc_app = format!("{}-fn-{}", safe_name, i);
            labels.insert("app".to_string(), svc_app.clone());

            // Build container for this function
            let img = f
                .provision_config
                .as_ref()
                .and_then(|p| p.container_image.as_deref())
                .unwrap_or("");
            let mut container = serde_json::json!({
                "name": "function",
                "image": img,
                "ports": [{"containerPort": fn_port}],
            });
            if ctx.enable_odgm_sidecar {
                let env = build_function_odgm_env_json(ctx)?; // Knative path default ODGM port 8081
                if !env.is_empty() {
                    container
                        .as_object_mut()
                        .unwrap()
                        .insert("env".into(), serde_json::Value::Array(env));
                }
            }

            // If there's a single function, keep the service name equal to the safe base
            // so tests that expect the DR-derived name continue to pass.
            let svc_name = if ctx.spec.functions.len() == 1 {
                safe_name.clone()
            } else {
                format!("{}-fn-{}", safe_name, i)
            };
            let kns = serde_json::json!({
                "apiVersion": "serving.knative.dev/v1",
                "kind": "Service",
                "metadata": {
                    "name": svc_name,
                    "labels": labels,
                    "annotations": annotations,
                    // OwnerReferences cannot be set via SSA on arbitrary resources easily without UIDs; leave to controller default GC by label
                },
                "spec": {
                    "template": {
                        "metadata": {
                            "labels": {
                                "app": svc_app,
                                "oaas.io/owner": safe_name,
                            }
                        },
                        "spec": {
                            "containers": [ container ]
                        }
                    }
                }
            });

            resources.push(RenderedResource::Other {
                api_version: "serving.knative.dev/v1".to_string(),
                kind: "Service".to_string(),
                manifest: kns,
            });
        }

        // Also render ODGM as separate Deployment/Service when enabled
        if ctx.enable_odgm_sidecar {
            // Re-use shared builder (omit owner refs to keep previous behaviour for Knative path)
            let (dep, svc) = odgm::build_odgm_resources(ctx, 1, None, false)?;
            resources.push(RenderedResource::Deployment(dep));
            resources.push(RenderedResource::Service(svc));
        }

        Ok(resources)
    }

    fn score(
        &self,
        env: &EnvironmentContext<'_>,
        nfr: Option<&crate::crd::class_runtime::NfrRequirementsSpec>,
    ) -> i32 {
        // Prefer knative for full/prod or datacenter; moderate for edge; low for dev
        let profile = env.profile.to_ascii_lowercase();
        let base = if env.is_datacenter
            || profile == "full"
            || profile == "prod"
            || profile == "production"
        {
            900_000
        } else if env.is_edge || profile == "edge" {
            600_000
        } else {
            100_000
        };
        let mut s = base;
        if let Some(n) = nfr {
            if n.min_throughput_rps.unwrap_or(0) >= 500 {
                s += 2;
            }
            if n.max_latency_ms.unwrap_or(u32::MAX) <= 100 {
                s += 2;
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::class_runtime::{
        ClassRuntimeSpec as DeploymentRecordSpec, FunctionSpec, OdgmConfigSpec,
    };
    use crate::templates::TemplateManager;
    use crate::templates::manager::TemplateError;
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
    fn knative_renders_kn_service_basic() {
        let spec = base_spec();
        let tpl = KnativeTemplate::default();
        let resources = tpl
            .render(&RenderContext {
                name: "class-a",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                router_service_name: None,
                router_service_port: None,
                spec: &spec,
            })
            .expect("render knative service");
        // Expect exactly one resource: Knative Service as Other
        assert_eq!(resources.len(), 1);
        match &resources[0] {
            RenderedResource::Other {
                api_version,
                kind,
                manifest,
            } => {
                assert_eq!(api_version, "serving.knative.dev/v1");
                assert_eq!(kind, "Service");
                assert_eq!(
                    manifest.get("kind").and_then(|v| v.as_str()).unwrap(),
                    "Service"
                );
                let name = manifest
                    .get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap();
                assert_eq!(name, "class-a");
            }
            _ => panic!("expected Other/Knative Service"),
        }
    }

    #[test]
    fn knative_renders_odgm_env_and_resources_when_enabled() {
        let mut spec = base_spec();
        spec.odgm_config = Some(OdgmConfigSpec {
            collections: Some(vec!["orders".into(), "users".into()]),
            partition_count: Some(1),
            replica_count: Some(1),
            shard_type: Some("mst".into()),
            ..Default::default()
        });
        let tpl = KnativeTemplate::default();
        let resources = tpl
            .render(&RenderContext {
                name: "class-b",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: true,
                profile: "full",
                router_service_name: None,
                router_service_port: None,
                spec: &spec,
            })
            .expect("render knative service with odgm");
        // Verify env injection on Knative container
        let kns_manifest = match &resources[0] {
            RenderedResource::Other { manifest, .. } => manifest,
            _ => panic!("expected knative service first"),
        };
        let containers = kns_manifest
            .get("spec")
            .and_then(|s| s.get("template"))
            .and_then(|t| t.get("spec"))
            .and_then(|sp| sp.get("containers"))
            .and_then(|c| c.as_array())
            .unwrap();
        let env = containers[0].get("env").and_then(|e| e.as_array()).unwrap();
        let mut names: Vec<_> = env
            .iter()
            .map(|e| e.get("name").unwrap().as_str().unwrap().to_string())
            .collect();
        names.sort();
        assert!(names.contains(&"ODGM_ENABLED".to_string()));
        assert!(names.contains(&"ODGM_SERVICE".to_string()));
        assert!(names.contains(&"ODGM_COLLECTION".to_string()));
    }

    #[test]
    fn template_manager_selects_knative_when_enabled_full_profile() {
        let spec = base_spec();
        let tm = TemplateManager::new(true /* include_knative */);
        // No ODGM for simplicity
        let res = tm
            .render_workload(RenderContext {
                name: "class-c",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                router_service_name: None,
                router_service_port: None,
                spec: &spec,
            })
            .expect("expected successful render");
        // Knative chosen -> first resource is Other/Knative Service
        matches!(res[0], RenderedResource::Other { .. });
    }

    #[test]
    fn template_manager_uses_k8s_deploy_when_knative_disabled() {
        let spec = base_spec();
        let tm = TemplateManager::new(false /* include_knative */);
        let res = tm
            .render_workload(RenderContext {
                name: "class-d",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                router_service_name: None,
                router_service_port: None,
                spec: &spec,
            })
            .expect("expected successful render");
        // Expect classic Deployment/Service (no Other)
        assert!(
            res.iter()
                .any(|r| matches!(r, RenderedResource::Deployment(_)))
        );
        assert!(
            res.iter()
                .any(|r| matches!(r, RenderedResource::Service(_)))
        );
        assert!(
            !res.iter()
                .any(|r| matches!(r, RenderedResource::Other { .. }))
        );
    }

    #[test]
    fn template_manager_errors_without_function_image() {
        let mut spec = base_spec();
        // Clear image to verify validation catches missing image
        spec.functions[0]
            .provision_config
            .as_mut()
            .unwrap()
            .container_image = None;
        let tm = TemplateManager::new(true);
        let err = tm
            .render_workload(RenderContext {
                name: "class-e",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                router_service_name: None,
                router_service_port: None,
                spec: &spec,
            })
            .expect_err("expected missing image error");
        matches!(err, TemplateError::MissingFunctionImage);
    }

    #[test]
    fn knative_manifest_contains_function_image() {
        let spec = base_spec();
        let tm = TemplateManager::new(true);
        let res = tm
            .render_workload(RenderContext {
                name: "class-f",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                router_service_name: None,
                router_service_port: None,
                spec: &spec,
            })
            .unwrap();
        let manifest = match &res[0] {
            RenderedResource::Other { manifest, .. } => manifest,
            _ => panic!("expected knative service manifest"),
        };
        let img = manifest
            .get("spec")
            .and_then(|s| s.get("template"))
            .and_then(|t| t.get("spec"))
            .and_then(|sp| sp.get("containers"))
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("image"))
            .and_then(|i| i.as_str())
            .unwrap();
        assert_eq!(img, "img:function");
    }
}
