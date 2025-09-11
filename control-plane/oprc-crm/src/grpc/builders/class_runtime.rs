use kube::Resource;
use std::collections::BTreeMap;

use crate::crd::class_runtime::{
    ClassRuntime, ClassRuntimeSpec, FunctionSpec, InvocationsSpec,
    OdgmConfigSpec,
};
use crate::grpc::helpers::{ANNO_CORRELATION_ID, LABEL_DEPLOYMENT_ID};
use oprc_grpc::proto::deployment::{
    DeploymentUnit, FunctionDeploymentSpec as GrpcFunction,
};
use oprc_models::ProvisionConfig;

/// Builder that converts a gRPC DeploymentUnit into a ClassRuntime CRD
pub struct ClassRuntimeBuilder {
    name: String,
    deployment_id: String,
    corr: Option<String>,
    du: DeploymentUnit,
}

impl ClassRuntimeBuilder {
    pub fn new(
        name: String,
        deployment_id: String,
        corr: Option<String>,
        du: DeploymentUnit,
    ) -> Self {
        Self {
            name,
            deployment_id,
            corr,
            du,
        }
    }

    pub fn build(self) -> ClassRuntime {
        let spec = ClassRuntimeSpec {
            package_class_key: Some(format!(
                "{}.{}",
                self.du.package_name, self.du.class_key
            )),
            odgm_config: self.build_odgm_config(),
            functions: self.build_functions(),
            ..Default::default()
        };

        let mut dr = ClassRuntime::new(&self.name, spec);
        let labels = dr.meta_mut().labels.get_or_insert_with(Default::default);
        labels.insert(LABEL_DEPLOYMENT_ID.into(), self.deployment_id.clone());
        if let Some(c) = self.corr {
            let ann = dr
                .meta_mut()
                .annotations
                .get_or_insert_with(Default::default);
            ann.insert(ANNO_CORRELATION_ID.into(), c);
        }
        dr
    }

    // DU-level NFR removed; keep a placeholder for future aggregation if needed

    fn build_odgm_config(&self) -> Option<OdgmConfigSpec> {
        // Prefer DU-provided ODGM config; fall back to minimal defaults when functions exist
        if let Some(cfg) = &self.du.odgm_config {
            let collections = if cfg.collections.is_empty() {
                None
            } else {
                Some(cfg.collections.clone())
            };
            let partition_count = cfg.partition_count.map(|v| v as i32);
            let replica_count = cfg.replica_count.map(|v| v as i32);
            let shard_type = cfg.shard_type.clone();

            let invocations = self.map_invocations_from_proto(cfg);
            let options = if cfg.options.is_empty() {
                None
            } else {
                // prost maps to std::collections::HashMap<String,String>
                let mut b = BTreeMap::new();
                for (k, v) in &cfg.options {
                    b.insert(k.clone(), v.clone());
                }
                Some(b)
            };

            return Some(OdgmConfigSpec {
                collections,
                partition_count,
                replica_count,
                shard_type,
                invocations,
                options,
                log: cfg.log.clone(),
            });
        }

        if self.du.functions.is_empty() {
            return None;
        }
        let collections = vec![self.name.clone()];
        let invocations = self.build_invocations();
        Some(OdgmConfigSpec {
            collections: Some(collections),
            partition_count: Some(1),
            replica_count: Some(1),
            shard_type: Some("mst".into()),
            invocations,
            options: None,
            log: None,
        })
    }

    fn map_invocations_from_proto(
        &self,
        cfg: &oprc_grpc::proto::deployment::OdgmConfig,
    ) -> Option<InvocationsSpec> {
        if cfg.invocations.is_none() {
            return None;
        }
        let inv = cfg.invocations.as_ref().unwrap();
        let mut routes = BTreeMap::new();
        for (k, v) in &inv.fn_routes {
            routes.insert(
                k.clone(),
                crate::crd::class_runtime::FunctionRoute {
                    url: v.url.clone(),
                    stateless: v.stateless,
                    standby: v.standby,
                    active_group: v.active_group.clone(),
                },
            );
        }
        Some(InvocationsSpec {
            fn_routes: routes,
            disabled_fn: inv.disabled_fn.clone(),
        })
    }

    fn build_invocations(&self) -> Option<InvocationsSpec> {
        if self.du.functions.is_empty() {
            return None;
        }
        let mut routes = BTreeMap::new();
        for f in &self.du.functions {
            let url = self.derive_fn_url(f);
            routes.insert(
                f.function_key.clone(),
                crate::crd::class_runtime::FunctionRoute {
                    url,
                    stateless: Some(true),
                    standby: Some(false),
                    active_group: vec![],
                },
            );
        }
        Some(InvocationsSpec {
            fn_routes: routes,
            disabled_fn: vec![],
        })
    }

    fn derive_fn_url(&self, f: &GrpcFunction) -> String {
        // Basic default; can be enhanced to use config or env
        format!("http://{}-{}-fn", self.name, f.function_key)
    }

    fn build_functions(&self) -> Vec<FunctionSpec> {
        self.du
            .functions
            .iter()
            .filter(|f| {
                f.provision_config
                    .as_ref()
                    .and_then(|p| p.container_image.as_ref())
                    .is_some()
            })
            .map(|f| self.map_function(f))
            .collect()
    }

    fn map_function(&self, f: &GrpcFunction) -> FunctionSpec {
        let provision = f.provision_config.as_ref().map(|p| ProvisionConfig {
            container_image: p.container_image.clone(),
            port: p.port.map(|v| v as u16),
            max_concurrency: p.max_concurrency,
            need_http2: p.need_http2,
            cpu_request: p.cpu_request.clone(),
            memory_request: p.memory_request.clone(),
            cpu_limit: p.cpu_limit.clone(),
            memory_limit: p.memory_limit.clone(),
            min_scale: p.min_scale,
            max_scale: p.max_scale,
        });

        FunctionSpec {
            function_key: f.function_key.clone(),
            description: f.description.clone(),
            available_location: f.available_location.clone(),
            qos_requirement: None,
            provision_config: provision,
            config: f.config.clone(),
        }
    }
}
