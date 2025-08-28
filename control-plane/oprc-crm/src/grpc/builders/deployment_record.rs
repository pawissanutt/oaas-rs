use kube::Resource;
use std::collections::{BTreeMap, HashMap};

use crate::crd::deployment_record::{
    DeploymentRecord, DeploymentRecordSpec, FunctionSpec, InvocationsSpec,
    NfrRequirementsSpec, OdgmConfigSpec,
};
use crate::grpc::helpers::{ANNO_CORRELATION_ID, LABEL_DEPLOYMENT_ID};
use oprc_grpc::proto::deployment::{
    DeploymentUnit, FunctionDeploymentSpec as GrpcFunction,
};
use oprc_models::ProvisionConfig;

pub struct DeploymentRecordBuilder {
    name: String,
    deployment_id: String,
    corr: Option<String>,
    du: DeploymentUnit,
}

impl DeploymentRecordBuilder {
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

    pub fn build(self) -> DeploymentRecord {
        let spec = DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: self.build_odgm_config(),
            functions: self.build_functions(),
            nfr_requirements: self.map_nfr_requirements(),
            nfr: None,
        };

        let mut dr = DeploymentRecord::new(&self.name, spec);
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

    fn map_nfr_requirements(&self) -> Option<NfrRequirementsSpec> {
        let Some(nfr) = &self.du.nfr_requirements else { return None; };
        Some(NfrRequirementsSpec {
            min_throughput_rps: nfr.min_throughput_rps,
            max_latency_ms: nfr.max_latency_ms,
            availability_pct: nfr.availability.map(|v| v as f32),
            consistency: None,
        })
    }

    fn build_odgm_config(&self) -> Option<OdgmConfigSpec> {
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
                crate::crd::deployment_record::FunctionRoute {
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
            .filter(|f| !f.image.is_empty())
            .map(|f| self.map_function(f))
            .collect()
    }

    fn map_function(&self, f: &GrpcFunction) -> FunctionSpec {
        let provision =
            f.resource_requirements.as_ref().map(|r| ProvisionConfig {
                cpu_request: Some(r.cpu_request.clone()),
                memory_request: Some(r.memory_request.clone()),
                cpu_limit: r.cpu_limit.clone(),
                memory_limit: r.memory_limit.clone(),
                container_image: if f.image.is_empty() {
                    None
                } else {
                    Some(f.image.clone())
                },
                ..Default::default()
            });

        FunctionSpec {
            function_key: f.function_key.clone(),
            replicas: f.replicas,
            container_image: if f.image.is_empty() {
                None
            } else {
                Some(f.image.clone())
            },
            description: None,
            available_location: None,
            qos_requirement: None,
            provision_config: provision,
            config: HashMap::new(),
        }
    }
}
