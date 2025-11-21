use super::manager::{
    dns1035_safe, EnvironmentContext, RenderContext, RenderedResource, Template,
};
use crate::templates::odgm;
use crate::templates::odgm::build_function_odgm_env_json;
use std::collections::HashSet;

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

            // Extract optional per-function Knative autoscaling + runtime config.
            // Only ProvisionConfig.{min_scale,max_scale}; we no longer read function config keys.
            // We do NOT inject defaults; only user-specified values.
            let mut revision_annotations: std::collections::BTreeMap<
                String,
                String,
            > = std::collections::BTreeMap::new();
            if let Some(ms) =
                f.provision_config.as_ref().and_then(|p| p.min_scale)
            {
                revision_annotations.insert(
                    "autoscaling.knative.dev/minScale".into(),
                    ms.to_string(),
                );
            }
            if let Some(mx) =
                f.provision_config.as_ref().and_then(|p| p.max_scale)
            {
                revision_annotations.insert(
                    "autoscaling.knative.dev/maxScale".into(),
                    mx.to_string(),
                );
            }
            // HTTP2 enablement from ProvisionConfig
            if f.provision_config
                .as_ref()
                .map(|p| p.need_http2)
                .unwrap_or(false)
            {
                revision_annotations.insert(
                    "networking.knative.dev/http2".into(),
                    "true".into(),
                );
            }

            let mut labels = std::collections::BTreeMap::new();
            labels.insert("oaas.io/owner".to_string(), safe_name.clone());
            let svc_app = format!("{}-fn-{}", safe_name, i);
            labels.insert("app".to_string(), svc_app.clone());
            labels.insert(
                "networking.knative.dev/visibility".to_string(),
                "cluster-local".to_string(),
            );

            // Build container for this function
            let img = f
                .provision_config
                .as_ref()
                .and_then(|p| p.container_image.as_deref())
                .unwrap_or("");
            let mut container = serde_json::json!({
                "name": "function",
                "image": img,
                "ports": [{"containerPort": fn_port, "name": "h2c", "protocol": "TCP"}],
            });
            // Add resources if specified in ProvisionConfig
            if let Some(pc) = f.provision_config.as_ref() {
                let mut requests = serde_json::Map::new();
                let mut limits = serde_json::Map::new();
                if let Some(cpu) = pc.cpu_request.as_ref() {
                    requests.insert(
                        "cpu".into(),
                        serde_json::Value::String(cpu.clone()),
                    );
                }
                if let Some(mem) = pc.memory_request.as_ref() {
                    requests.insert(
                        "memory".into(),
                        serde_json::Value::String(mem.clone()),
                    );
                }
                if let Some(cpu) = pc.cpu_limit.as_ref() {
                    limits.insert(
                        "cpu".into(),
                        serde_json::Value::String(cpu.clone()),
                    );
                }
                if let Some(mem) = pc.memory_limit.as_ref() {
                    limits.insert(
                        "memory".into(),
                        serde_json::Value::String(mem.clone()),
                    );
                }
                let mut res_obj = serde_json::Map::new();
                if !requests.is_empty() {
                    res_obj.insert(
                        "requests".into(),
                        serde_json::Value::Object(requests),
                    );
                }
                if !limits.is_empty() {
                    res_obj.insert(
                        "limits".into(),
                        serde_json::Value::Object(limits),
                    );
                }
                if !res_obj.is_empty() {
                    container.as_object_mut().unwrap().insert(
                        "resources".into(),
                        serde_json::Value::Object(res_obj),
                    );
                }
            }
            // Add function config as env
            if !f.config.is_empty() {
                let cfg_env: Vec<serde_json::Value> = f
                    .config
                    .iter()
                    .map(|(k, v)| {
                        serde_json::json!({
                            "name": k,
                            "value": v,
                        })
                    })
                    .collect();
                container
                    .as_object_mut()
                    .unwrap()
                    .insert("env".into(), serde_json::Value::Array(cfg_env));
            }
            if ctx.enable_odgm_sidecar {
                let mut env = build_function_odgm_env_json(ctx)?; // Knative path default ODGM port 8081
                                                                  // Exclude ODGM_COLLECTION to avoid frequent spec churn (large JSON with dynamic shard assignment IDs)
                env.retain(|e| {
                    e.get("name").and_then(|v| v.as_str())
                        != Some("ODGM_COLLECTION")
                });
                if !env.is_empty() {
                    let obj = container.as_object_mut().unwrap();
                    if let Some(existing) = obj.get_mut("env") {
                        if let Some(arr) = existing.as_array_mut() {
                            arr.extend(env);
                        }
                    } else {
                        obj.insert("env".into(), serde_json::Value::Array(env));
                    }
                }
            }

            // Inject Zenoh router config if available
            if let Some(router_name) = ctx.router_service_name.as_ref() {
                let router_port = ctx.router_service_port.unwrap_or(17447);
                let zenoh_env = vec![
                    serde_json::json!({
                        "name": "OPRC_ZENOH_MODE",
                        "value": "client",
                    }),
                    serde_json::json!({
                        "name": "OPRC_ZENOH_PEERS",
                        "value": format!("tcp/{}:{}", router_name, router_port),
                    }),
                ];

                let obj = container.as_object_mut().unwrap();
                if let Some(existing) = obj.get_mut("env") {
                    if let Some(arr) = existing.as_array_mut() {
                        arr.extend(zenoh_env);
                    }
                } else {
                    obj.insert(
                        "env".into(),
                        serde_json::Value::Array(zenoh_env),
                    );
                }
            }

            // Inject OTEL config if enabled
            if ctx.otel_enabled {
                let otel_env = vec![
                    serde_json::json!({
                        "name": "OTEL_EXPORTER_OTLP_ENDPOINT",
                        "value": ctx.otel_endpoint,
                    }),
                    serde_json::json!({
                        "name": "OTEL_SERVICE_NAME",
                        "value": ctx.name,
                    }),
                ];

                let obj = container.as_object_mut().unwrap();
                if let Some(existing) = obj.get_mut("env") {
                    if let Some(arr) = existing.as_array_mut() {
                        arr.extend(otel_env);
                    }
                } else {
                    obj.insert(
                        "env".into(),
                        serde_json::Value::Array(otel_env),
                    );
                }
            }

            // Deterministically sort env array (and deduplicate by name keeping first)
            if let Some(env_val) =
                container.as_object_mut().unwrap().get_mut("env")
            {
                if let Some(arr) = env_val.as_array_mut() {
                    arr.sort_by(|a, b| {
                        let an = a
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let bn = b
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        an.cmp(bn)
                    });
                    let mut seen: HashSet<String> = HashSet::new();
                    arr.retain(|e| {
                        if let Some(n) = e.get("name").and_then(|v| v.as_str())
                        {
                            if seen.contains(n) {
                                return false;
                            }
                            seen.insert(n.to_string());
                            true
                        } else {
                            true
                        }
                    });
                }
            }

            // If there's a single function, keep the service name equal to the safe base
            // so tests that expect the DR-derived name continue to pass.
            let svc_name = if ctx.spec.functions.len() == 1 {
                safe_name.clone()
            } else {
                format!("{}-fn-{}", safe_name, i)
            };
            let mut kns = serde_json::json!({
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
            // containerConcurrency from ProvisionConfig.max_concurrency (non-zero)
            if let Some(pc) = f.provision_config.as_ref() {
                if pc.max_concurrency > 0 {
                    kns["spec"]["template"]["spec"]
                        .as_object_mut()
                        .unwrap()
                        .insert(
                            "containerConcurrency".into(),
                            serde_json::json!(pc.max_concurrency),
                        );
                }
            }
            if !revision_annotations.is_empty() {
                kns["spec"]["template"]["metadata"]
                    .as_object_mut()
                    .unwrap()
                    .insert(
                        "annotations".into(),
                        serde_json::to_value(&revision_annotations).unwrap(),
                    );
            }

            resources.push(RenderedResource::Other {
                api_version: "serving.knative.dev/v1".to_string(),
                kind: "Service".to_string(),
                manifest: kns,
            });
        }

        // Also render ODGM as separate Deployment/Service when enabled
        if ctx.enable_odgm_sidecar {
            // Re-use shared builder (omit owner refs to keep previous behaviour for Knative path)
            let (dep, svc) = odgm::build_odgm_resources(
                ctx,
                1,
                ctx.odgm_image_override,
                false,
            )?;
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
        ClassRuntimeSpec, FunctionSpec, OdgmConfigSpec,
    };
    use crate::templates::manager::TemplateError;
    use crate::templates::TemplateManager;
    use oprc_models::ProvisionConfig;

    fn base_spec() -> ClassRuntimeSpec {
        ClassRuntimeSpec {
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
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
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
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: true,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
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
        // ODGM_COLLECTION intentionally excluded in Knative path
        assert!(!names.contains(&"ODGM_COLLECTION".to_string()));
    }

    #[test]
    fn knative_env_order_stable_sorted_by_name() {
        // Build spec with intentionally unsorted env config keys
        let mut spec = base_spec();
        if let Some(f) = spec.functions.first_mut() {
            f.config.insert("Z_KEY".into(), "z".into());
            f.config.insert("A_KEY".into(), "a".into());
            f.config.insert("M_KEY".into(), "m".into());
        }
        let tpl = KnativeTemplate::default();
        let resources1 = tpl
            .render(&RenderContext {
                name: "class-c",
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
                spec: &spec,
            })
            .expect("render knative service #1");
        let resources2 = tpl
            .render(&RenderContext {
                name: "class-c",
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
                spec: &spec,
            })
            .expect("render knative service #2");
        // Extract env arrays from both renders
        let extract_env = |res: &Vec<RenderedResource>| -> Vec<String> {
            let manifest = match &res[0] {
                RenderedResource::Other { manifest, .. } => manifest,
                _ => panic!("expected Other"),
            };
            manifest["spec"]["template"]["spec"]["containers"][0]["env"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e["name"].as_str().unwrap().to_string())
                .collect()
        };
        let env1 = extract_env(&resources1);
        let env2 = extract_env(&resources2);
        assert_eq!(env1, env2, "env ordering should be stable across renders");
        // Ensure sorted order (A_KEY before M_KEY before Z_KEY)
        let a_pos = env1.iter().position(|n| n == "A_KEY").unwrap();
        let m_pos = env1.iter().position(|n| n == "M_KEY").unwrap();
        let z_pos = env1.iter().position(|n| n == "Z_KEY").unwrap();
        assert!(
            a_pos < m_pos && m_pos < z_pos,
            "env vars not sorted lexicographically"
        );
    }

    #[test]
    fn template_manager_selects_knative_when_enabled_full_profile() {
        let spec = base_spec();
        let tm = TemplateManager::new(true /* include_knative */);
        // No ODGM for simplicity
        let res = tm
            .render_workload(RenderContext {
                name: "class-c",
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
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
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
                spec: &spec,
            })
            .expect("expected successful render");
        // Expect classic Deployment/Service (no Other)
        assert!(res
            .iter()
            .any(|r| matches!(r, RenderedResource::Deployment(_))));
        assert!(res
            .iter()
            .any(|r| matches!(r, RenderedResource::Service(_))));
        assert!(!res
            .iter()
            .any(|r| matches!(r, RenderedResource::Other { .. })));
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
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
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
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
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

        // No scaling annotations should be present when not specified
        let tmpl_meta = manifest
            .get("spec")
            .and_then(|s| s.get("template"))
            .and_then(|t| t.get("metadata"))
            .unwrap();
        if let Some(ann) = tmpl_meta.get("annotations") {
            assert!(ann.get("autoscaling.knative.dev/minScale").is_none());
            assert!(ann.get("autoscaling.knative.dev/maxScale").is_none());
            assert!(ann.get("autoscaling.knative.dev/target").is_none());
        }
    }

    #[test]
    fn knative_manifest_respects_scaling_overrides() {
        let mut spec = base_spec();
        spec.functions[0]
            .provision_config
            .as_mut()
            .unwrap()
            .min_scale = Some(2);
        spec.functions[0]
            .provision_config
            .as_mut()
            .unwrap()
            .max_scale = Some(10);
        let tm = TemplateManager::new(true);
        let res = tm
            .render_workload(RenderContext {
                name: "class-g",
                namespace: "default",
                owner_api_version: "oaas.io/v1alpha1",
                owner_kind: "ClassRuntime",
                owner_uid: None,
                enable_odgm_sidecar: false,
                profile: "full",
                odgm_image_override: None,
                odgm_pull_policy_override: None,
                router_service_name: None,
                router_service_port: None, otel_enabled: false, otel_endpoint: "",
                spec: &spec,
            })
            .unwrap();
        let manifest = match &res[0] {
            RenderedResource::Other { manifest, .. } => manifest,
            _ => panic!("expected knative service manifest"),
        };
        let ann = manifest
            .get("spec")
            .and_then(|s| s.get("template"))
            .and_then(|t| t.get("metadata"))
            .and_then(|m| m.get("annotations"))
            .expect("annotations present");
        assert_eq!(
            ann.get("autoscaling.knative.dev/minScale")
                .unwrap()
                .as_str()
                .unwrap(),
            "2"
        );
        assert_eq!(
            ann.get("autoscaling.knative.dev/maxScale")
                .unwrap()
                .as_str()
                .unwrap(),
            "10"
        );
        assert!(ann.get("autoscaling.knative.dev/target").is_none());
    }
}
