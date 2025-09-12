use super::requirements::DeploymentRequirements;
use crate::services::deployment::generate_shard_assignments_spec;
use chrono::Utc;
use oprc_grpc::types as grpc_types;
use oprc_models::{OClass, OClassDeployment};

pub fn gen_safe_deployment_id() -> String {
    loop {
        let id = nanoid::nanoid!();
        let first_ok = id
            .chars()
            .next()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false);
        let last_ok = id
            .chars()
            .last()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false);
        if first_ok && last_ok {
            return id;
        }
    }
}

pub fn create_deployment_units_for_env(
    _class: &OClass,
    deployment: &OClassDeployment,
    env_name: &str,
    requirements: &DeploymentRequirements,
) -> grpc_types::DeploymentUnit {
    let functions: Vec<grpc_types::FunctionDeploymentSpec> = deployment
        .functions
        .iter()
        .map(|f| {
            let mut pc_model = f.provision_config.clone().unwrap_or_default();
            pc_model.min_scale = Some(requirements.target_replicas.max(1));
            let provision_config = Some(grpc_types::ProvisionConfig {
                container_image: pc_model.container_image.clone(),
                port: pc_model.port.map(|p| p as u32),
                max_concurrency: pc_model.max_concurrency,
                need_http2: pc_model.need_http2,
                cpu_request: pc_model.cpu_request.clone(),
                memory_request: pc_model.memory_request.clone(),
                cpu_limit: pc_model.cpu_limit.clone(),
                memory_limit: pc_model.memory_limit.clone(),
                min_scale: pc_model.min_scale,
                max_scale: pc_model.max_scale,
            });
            let nfr = &deployment.nfr_requirements;
            let nfr_requirements = Some(grpc_types::NfrRequirements {
                min_throughput_rps: nfr.min_throughput_rps,
                availability: nfr.availability,
                cpu_utilization_target: nfr.cpu_utilization_target,
                ..Default::default()
            });
            grpc_types::FunctionDeploymentSpec {
                function_key: f.function_key.clone(),
                description: f.description.clone(),
                available_location: f.available_location.clone(),
                nfr_requirements,
                provision_config,
                config: f.config.clone(),
            }
        })
        .collect();

    let odgm_config = deployment.odgm.as_ref().map(|o| {
        let ids_for_env =
            o.env_node_ids.get(env_name).cloned().unwrap_or_default();
        let mut env_map: std::collections::HashMap<
            String,
            grpc_types::OdgmNodeIds,
        > = std::collections::HashMap::new();
        for (k, v) in &o.env_node_ids {
            env_map
                .insert(k.clone(), grpc_types::OdgmNodeIds { ids: v.clone() });
        }
        let mut collection_assignments: std::collections::HashMap<
            String,
            grpc_types::CollectionShardAssignments,
        > = std::collections::HashMap::new();
        if !o.collections.is_empty() {
            let partitions = o.partition_count.unwrap_or(1).max(1);
            let replicas = o
                .replica_count
                .unwrap_or(requirements.target_replicas)
                .max(1);
            let mut all_nodes: Vec<u64> = o
                .env_node_ids
                .values()
                .flat_map(|v| v.iter().copied())
                .collect();
            all_nodes.sort_unstable();
            all_nodes.dedup();
            if !all_nodes.is_empty() {
                for col in &o.collections {
                    let assigns = generate_shard_assignments_spec(
                        partitions as usize,
                        replicas as usize,
                        &all_nodes,
                    );
                    collection_assignments.insert(
                        col.clone(),
                        grpc_types::CollectionShardAssignments {
                            assignments: assigns,
                        },
                    );
                }
            }
        }
        grpc_types::OdgmConfig {
            collections: o.collections.clone(),
            partition_count: o.partition_count,
            replica_count: o
                .replica_count
                .or(Some(requirements.target_replicas)),
            shard_type: o.shard_type.clone(),
            invocations: None,
            options: std::collections::HashMap::new(),
            log: o.log.clone(),
            env_node_ids: env_map,
            odgm_node_id: ids_for_env.first().cloned(),
            collection_assignments,
        }
    });

    grpc_types::DeploymentUnit {
        id: gen_safe_deployment_id(),
        package_name: deployment.package_name.clone(),
        class_key: deployment.class_key.clone(),
        functions,
        target_env: env_name.to_string(),
        created_at: Some(grpc_types::Timestamp {
            seconds: Utc::now().timestamp(),
            nanos: Utc::now().timestamp_subsec_nanos() as i32,
        }),
        odgm_config,
    }
}

pub fn create_deployment_units(
    class: &OClass,
    deployment: &OClassDeployment,
    requirements: &DeploymentRequirements,
) -> Vec<grpc_types::DeploymentUnit> {
    deployment
        .target_envs
        .iter()
        .map(|env| {
            create_deployment_units_for_env(
                class,
                deployment,
                env,
                requirements,
            )
        })
        .collect()
}
