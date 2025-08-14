use super::manager::{
    EnvironmentContext, RenderContext, RenderedResource, Template,
};

#[derive(Clone, Debug, Default)]
pub struct KnativeTemplate;

impl Template for KnativeTemplate {
    fn name(&self) -> &'static str {
        "knative"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["kn", "knsvc"]
    }

    fn render(&self, ctx: &RenderContext<'_>) -> Vec<RenderedResource> {
        // Render a Knative Service instead of Deployment/Service.
        // We still propagate ODGM env to function via annotations/env when applicable in a future pass.
        let fn_img = ctx
            .spec
            .function
            .as_ref()
            .and_then(|f| f.image.as_deref())
            .unwrap_or("ghcr.io/pawissanutt/oprc-function:latest");
        let fn_port = ctx
            .spec
            .function
            .as_ref()
            .and_then(|f| f.port)
            .unwrap_or(8080);

        let mut annotations = std::collections::BTreeMap::new();
        // Basic Prometheus scrape hints (Knative autoscaler/metrics integrations may pick these up)
        annotations
            .insert("prometheus.io/scrape".to_string(), "true".to_string());
        annotations
            .insert("prometheus.io/port".to_string(), fn_port.to_string());
        annotations
            .insert("prometheus.io/path".to_string(), "/metrics".to_string());

        let mut labels = std::collections::BTreeMap::new();
        labels.insert("oaas.io/owner".to_string(), ctx.name.to_string());
        labels.insert("app".to_string(), ctx.name.to_string());

        // Knative Service manifest as unstructured JSON for dynamic application
        // Build Knative Service container with optional ODGM envs
        let mut container = serde_json::json!({
            "name": "function",
            "image": fn_img,
            "ports": [{"containerPort": fn_port}],
        });
        if ctx.enable_odgm_sidecar {
            let odgm_name = format!("{}-odgm", ctx.name);
            let odgm_port = 8081;
            let odgm_service = format!("{}-svc:{}", odgm_name, odgm_port);
            let mut env: Vec<serde_json::Value> = vec![
                serde_json::json!({"name": "ODGM_ENABLED", "value": "true"}),
                serde_json::json!({"name": "ODGM_SERVICE", "value": odgm_service}),
            ];
            if let Some(cols) = ctx
                .spec
                .odgm_config
                .as_ref()
                .and_then(|o| o.collections.as_ref())
            {
                if !cols.is_empty() {
                    env.push(serde_json::json!({
                        "name": "ODGM_COLLECTIONS",
                        "value": cols.join(","),
                    }));
                }
            }
            container
                .as_object_mut()
                .unwrap()
                .insert("env".into(), serde_json::Value::Array(env));
        }

        let kns = serde_json::json!({
            "apiVersion": "serving.knative.dev/v1",
            "kind": "Service",
            "metadata": {
                "name": ctx.name,
                "labels": labels,
                "annotations": annotations,
                // OwnerReferences cannot be set via SSA on arbitrary resources easily without UIDs; leave to controller default GC by label
            },
            "spec": {
                "template": {
                    "metadata": {
                        "labels": {
                            "app": ctx.name,
                            "oaas.io/owner": ctx.name,
                        }
                    },
                    "spec": {
                        "containers": [ container ]
                    }
                }
            }
        });

        let mut resources = vec![RenderedResource::Other {
            api_version: "serving.knative.dev/v1".to_string(),
            kind: "Service".to_string(),
            manifest: kns,
        }];

        // Also render ODGM as separate Deployment/Service when enabled
        if ctx.enable_odgm_sidecar {
            use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
            use k8s_openapi::api::core::v1::{
                Container as KContainer, EnvVar, PodSpec, PodTemplateSpec,
                Service as KService, ServicePort, ServiceSpec,
            };
            use k8s_openapi::apimachinery::pkg::apis::meta::v1::{
                LabelSelector, ObjectMeta,
            };
            use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;

            let odgm_name = format!("{}-odgm", ctx.name);
            let mut odgm_lbls = std::collections::BTreeMap::new();
            odgm_lbls.insert("app".to_string(), odgm_name.clone());
            odgm_lbls.insert("oaas.io/owner".to_string(), ctx.name.to_string());
            let odgm_labels = Some(odgm_lbls.clone());
            let odgm_selector = LabelSelector {
                match_labels: odgm_labels.clone(),
                ..Default::default()
            };
            let odgm_img = "ghcr.io/pawissanutt/oprc-odgm:latest";
            let odgm_port = 8081;

            let mut odgm_container = KContainer {
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

            let odgm_deployment = Deployment {
                metadata: ObjectMeta {
                    name: Some(odgm_name.clone()),
                    labels: odgm_labels.clone(),
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
            let odgm_service = KService {
                metadata: ObjectMeta {
                    name: Some(odgm_svc_name),
                    labels: odgm_labels.clone(),
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

    fn score(
        &self,
        env: &EnvironmentContext<'_>,
        nfr: Option<&crate::crd::deployment_record::NfrRequirementsSpec>,
    ) -> i32 {
        // Prefer knative for full/prod when available; moderate for edge; low for dev
        let base = match env.profile.to_ascii_lowercase().as_str() {
            "full" | "prod" | "production" => 900_000,
            "edge" => 600_000,
            _ => 100_000,
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
    use crate::crd::deployment_record::{
        DeploymentRecordSpec, FunctionSpec, OdgmConfigSpec,
    };
    use crate::templates::TemplateManager;

    fn base_spec() -> DeploymentRecordSpec {
        DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            function: Some(FunctionSpec {
                image: Some("img:function".into()),
                port: Some(8080),
            }),
            nfr_requirements: None,
            nfr: None,
        }
    }

    #[test]
    fn knative_renders_kn_service_basic() {
        let spec = base_spec();
        let tpl = KnativeTemplate::default();
        let resources = tpl.render(&RenderContext {
            name: "class-a",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "DeploymentRecord",
            owner_uid: None,
            enable_odgm_sidecar: false,
            profile: "full",
            spec: &spec,
        });
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
        });
        let tpl = KnativeTemplate::default();
        let resources = tpl.render(&RenderContext {
            name: "class-b",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "DeploymentRecord",
            owner_uid: None,
            enable_odgm_sidecar: true,
            profile: "full",
            spec: &spec,
        });
        // Expect Knative Service + ODGM Deployment + ODGM Service
        assert_eq!(resources.len(), 3);
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
        assert!(names.contains(&"ODGM_COLLECTIONS".to_string()));
    }

    #[test]
    fn template_manager_selects_knative_when_enabled_full_profile() {
        let spec = base_spec();
        let tm = TemplateManager::new(true /* include_knative */);
        // No ODGM for simplicity
        let res = tm.render_workload(RenderContext {
            name: "class-c",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "DeploymentRecord",
            owner_uid: None,
            enable_odgm_sidecar: false,
            profile: "full",
            spec: &spec,
        });
        // Knative chosen -> first resource is Other/Knative Service
        matches!(res[0], RenderedResource::Other { .. });
    }

    #[test]
    fn template_manager_uses_k8s_deploy_when_knative_disabled() {
        let spec = base_spec();
        let tm = TemplateManager::new(false /* include_knative */);
        let res = tm.render_workload(RenderContext {
            name: "class-d",
            owner_api_version: "oaas.io/v1alpha1",
            owner_kind: "DeploymentRecord",
            owner_uid: None,
            enable_odgm_sidecar: false,
            profile: "full",
            spec: &spec,
        });
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
}
