use super::{
    requirements::calculate_requirements, units::create_deployment_units,
};
use crate::{
    config::DeploymentPolicyConfig,
    crm::CrmManager,
    errors::{DeploymentError, PackageManagerError},
    models::{DeploymentFilter, DeploymentId},
};
use chrono::Utc;
use oprc_grpc::types as grpc_types;
use oprc_models::{DeploymentStatusSummary, OClass, OClassDeployment};
use std::sync::Arc;
use tracing::{error, info, warn};

pub struct DeploymentService {
    pub(crate) storage: Arc<dyn oprc_cp_storage::DeploymentStorage>,
    pub(crate) crm_manager: Arc<CrmManager>,
    pub(crate) policy: DeploymentPolicyConfig,
}

impl DeploymentService {
    pub fn new(
        storage: Arc<dyn oprc_cp_storage::DeploymentStorage>,
        crm_manager: Arc<CrmManager>,
        policy: DeploymentPolicyConfig,
    ) -> Self {
        Self {
            storage,
            crm_manager,
            policy,
        }
    }

    pub async fn deploy_class(
        &self,
        class: &OClass,
        deployment: &OClassDeployment,
    ) -> Result<DeploymentId, PackageManagerError> {
        info!(
            "Deploying class {} from package {}",
            class.key, deployment.package_name
        );
        let candidate_envs = self.select_candidate_envs(deployment).await?;
        let temp = self.temp_deployment_with_envs(deployment, &candidate_envs);
        let requirements =
            calculate_requirements(&self.crm_manager, class, &temp).await?;
        info!("Calculated deployment requirements: {:?}", requirements);
        let (selected_envs, env_avail) = self
            .select_environments(
                deployment,
                &candidate_envs,
                requirements.target_replicas,
            )
            .await?;
        let achieved = self.compute_achieved_quorum(&selected_envs, &env_avail);
        let mut effective = self.prepare_effective_deployment(
            deployment,
            &selected_envs,
            achieved,
        );
        self.ensure_odgm_node_ids(&mut effective, &selected_envs);
        let units = create_deployment_units(class, &effective, &requirements);
        info!("Created {} deployment units", units.len());
        effective.updated_at = Some(Utc::now());
        self.storage.store_deployment(&effective).await?;
        let successes = self
            .dispatch_units_with_retry(&units, &deployment.key)
            .await;
        self.finalize_dispatch(successes, &effective, &deployment.key)
    }

    async fn select_candidate_envs(
        &self,
        deployment: &OClassDeployment,
    ) -> Result<Vec<String>, PackageManagerError> {
        let all_clusters = self.crm_manager.list_clusters().await;
        let candidate_envs: Vec<String> = if deployment.target_envs.is_empty() {
            if deployment.available_envs.is_empty() {
                all_clusters.clone()
            } else {
                deployment
                    .available_envs
                    .iter()
                    .filter(|c| all_clusters.contains(c))
                    .cloned()
                    .collect()
            }
        } else {
            deployment.target_envs.clone()
        };
        if candidate_envs.is_empty() {
            return Err(DeploymentError::Invalid("No eligible environments found (check available_envs or cluster config)".into()).into());
        }
        Ok(candidate_envs)
    }

    fn temp_deployment_with_envs(
        &self,
        deployment: &OClassDeployment,
        envs: &[String],
    ) -> OClassDeployment {
        let mut tmp = deployment.clone();
        tmp.target_envs = envs.to_vec();
        tmp
    }

    async fn select_environments(
        &self,
        deployment: &OClassDeployment,
        candidate_envs: &[String],
        target_replicas: u32,
    ) -> Result<(Vec<String>, Vec<(String, f64)>), PackageManagerError> {
        let mut env_avail: Vec<(String, f64)> = Vec::new();
        for c in candidate_envs {
            match self.crm_manager.get_cluster_health(c).await {
                Ok(h) => {
                    let p = if let Some(av) = h.availability {
                        av
                    } else if let (Some(r), Some(n)) =
                        (h.ready_nodes, h.node_count)
                    {
                        if n == 0 {
                            0.0
                        } else {
                            (r as f64 / n as f64).clamp(0.0, 1.0)
                        }
                    } else {
                        0.0
                    };
                    env_avail.push((c.clone(), p.clamp(0.0, 1.0)));
                }
                Err(_) => env_avail.push((c.clone(), 0.0)),
            }
        }
        env_avail.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut selected_envs: Vec<String> = env_avail
            .iter()
            .take((target_replicas as usize).max(1))
            .map(|(n, _)| n.clone())
            .collect();
        if !deployment.target_envs.is_empty() {
            selected_envs = deployment.target_envs.clone();
        }
        Ok((selected_envs, env_avail))
    }

    fn compute_achieved_quorum(
        &self,
        selected_envs: &[String],
        env_avail: &[(String, f64)],
    ) -> f64 {
        let mut selected_avails: Vec<f64> = selected_envs
            .iter()
            .filter_map(|n| {
                env_avail.iter().find(|(cn, _)| cn == n).map(|(_, p)| *p)
            })
            .collect();
        selected_avails.sort_by(|a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        });
        if selected_avails.is_empty() {
            0.0
        } else {
            cluster_quorum_availability(&selected_avails)
        }
    }

    fn prepare_effective_deployment(
        &self,
        deployment: &OClassDeployment,
        selected_envs: &[String],
        achieved: f64,
    ) -> OClassDeployment {
        let mut effective = deployment.clone();
        if deployment.target_envs.is_empty() {
            effective.target_envs = selected_envs.to_vec();
        }
        if let Some(odgm) = effective.odgm.as_mut() {
            odgm.replica_count = Some(selected_envs.len() as u32);
        }
        effective.status = Some(DeploymentStatusSummary {
            replication_factor: selected_envs.len() as u32,
            selected_envs: selected_envs.to_vec(),
            achieved_quorum_availability: Some(achieved),
            last_error: None,
        });
        effective
    }

    fn ensure_odgm_node_ids(
        &self,
        effective: &mut OClassDeployment,
        selected_envs: &[String],
    ) {
        if let Some(odgm) = effective.odgm.as_mut() {
            if odgm.env_node_ids.is_empty() {
                for env in selected_envs {
                    let node_id: u64 = rand::random();
                    odgm.env_node_ids.insert(env.clone(), vec![node_id]);
                }
            }
        }
    }

    async fn dispatch_units_with_retry(
        &self,
        units: &[grpc_types::DeploymentUnit],
        deployment_key: &str,
    ) -> Vec<(String, String)> {
        let mut successes = Vec::new();
        for unit in units.iter() {
            let cluster_name = unit.target_env.clone();
            let mut attempt = 0u32;
            let mut last_err: Option<String> = None;
            loop {
                attempt += 1;
                match self.crm_manager.get_client(&cluster_name).await {
                    Ok(crm_client) => {
                        match crm_client.deploy(unit.clone()).await {
                            Ok(response) => {
                                info!(cluster=%cluster_name, id=%response.id, attempts=%attempt, "Deploy succeeded");
                                if let Err(e) = self
                                    .storage
                                    .save_cluster_mapping(
                                        deployment_key,
                                        &cluster_name,
                                        &response.id,
                                    )
                                    .await
                                {
                                    error!(cluster=%cluster_name, error=%e, "Failed saving cluster mapping");
                                }
                                successes.push((
                                    cluster_name.clone(),
                                    response.id.clone(),
                                ));
                                break;
                            }
                            Err(e) => {
                                error!(cluster=%cluster_name, attempt=%attempt, error=%e, "Deploy attempt failed");
                                last_err = Some(e.to_string());
                            }
                        }
                    }
                    Err(e) => {
                        error!(cluster=%cluster_name, attempt=%attempt, error=%e, "CRM client acquisition failed");
                        last_err = Some(e.to_string());
                    }
                }
                if attempt > self.policy.max_retries {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(
                    200 * attempt as u64,
                ))
                .await;
            }
            if let Some(err) = last_err {
                warn!(cluster=%cluster_name, error=%err, "Giving up after retries");
            }
        }
        successes
    }

    fn finalize_dispatch(
        &self,
        successes: Vec<(String, String)>,
        effective: &OClassDeployment,
        deployment_key: &str,
    ) -> Result<DeploymentId, PackageManagerError> {
        let total_clusters = effective.target_envs.len();
        if successes.len() == total_clusters {
            if let Some((_, id)) = successes.first() {
                return Ok(DeploymentId::from_string(id.clone()));
            }
        }
        if successes.is_empty() {
            warn!(deployment=%deployment_key, "All cluster deployments failed");
            return Err(DeploymentError::Invalid(
                "Deployment failed in all clusters".into(),
            )
            .into());
        }
        warn!(deployment=%deployment_key, successes=%successes.len(), total=%total_clusters, rollback=%self.policy.rollback_on_partial, "Partial deployment");
        if self.policy.rollback_on_partial {
            self.rollback(successes, deployment_key)?;
            return Err(DeploymentError::Invalid(
                "Deployment rolled back due to partial failure".into(),
            )
            .into());
        }
        Ok(DeploymentId::from_string(successes[0].1.clone()))
    }

    fn rollback(
        &self,
        successes: Vec<(String, String)>,
        deployment_key: &str,
    ) -> Result<(), PackageManagerError> {
        tokio::spawn({
            let crm = self.crm_manager.clone();
            let storage = self.storage.clone();
            let key = deployment_key.to_string();
            async move {
                for (cluster, dep_id) in successes.iter() {
                    match crm.get_client(cluster).await {
                        Ok(c) => {
                            if let Err(e) = c.delete_deployment(dep_id).await {
                                warn!(cluster=%cluster, id=%dep_id, error=%e, "Rollback delete failed");
                            }
                        }
                        Err(e) => {
                            warn!(cluster=%cluster, error=%e, "Rollback client acquisition failed")
                        }
                    }
                }
                if let Err(e) = storage.remove_cluster_mappings(&key).await {
                    warn!(deployment=%key, error=%e, "Failed clearing cluster mappings after rollback");
                }
            }
        });
        Ok(())
    }

    pub async fn list_deployments(
        &self,
        filter: DeploymentFilter,
    ) -> Result<Vec<OClassDeployment>, PackageManagerError> {
        info!("Listing deployments with filter: {:?}", filter);
        let storage_filter = oprc_models::DeploymentFilter {
            package_name: filter.package_name,
            class_key: filter.class_key,
            target_env: filter.target_env,
            condition: None,
        };
        let mut deployments =
            self.storage.list_deployments(storage_filter).await?;
        if let Some(offset) = filter.offset {
            deployments = deployments.into_iter().skip(offset).collect();
        }
        if let Some(limit) = filter.limit {
            deployments.truncate(limit);
        }
        Ok(deployments)
    }

    pub async fn get_deployment(
        &self,
        key: &str,
    ) -> Result<Option<OClassDeployment>, PackageManagerError> {
        info!("Getting deployment: {}", key);
        let deployment = self.storage.get_deployment(key).await?;
        Ok(deployment)
    }

    pub async fn delete_deployment(
        &self,
        key: &str,
    ) -> Result<(), PackageManagerError> {
        info!("Deleting deployment: {}", key);
        let deployment = self
            .storage
            .get_deployment(key)
            .await?
            .ok_or_else(|| DeploymentError::NotFound(key.to_string()))?;
        let cluster_id_map = self
            .storage
            .get_cluster_mappings(&deployment.key)
            .await
            .unwrap_or_default();
        let envs: Vec<String> = if let Some(status) = &deployment.status {
            if !status.selected_envs.is_empty() {
                status.selected_envs.clone()
            } else {
                deployment.target_envs.clone()
            }
        } else {
            deployment.target_envs.clone()
        };
        for cluster_name in &envs {
            match self.crm_manager.get_client(cluster_name).await {
                Ok(crm_client) => {
                    let cid = cluster_id_map
                        .get(cluster_name)
                        .cloned()
                        .unwrap_or_else(|| key.to_string());
                    if let Err(e) = crm_client.delete_deployment(&cid).await {
                        warn!(
                            "Failed to delete deployment (id {} fallback key {}) from cluster {}: {}",
                            cid, key, cluster_name, e
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to get CRM client for cluster {}: {}",
                        cluster_name, e
                    );
                }
            }
        }
        self.storage.delete_deployment(key).await?;
        if let Err(e) = self.storage.remove_cluster_mappings(key).await {
            warn!("Failed to remove cluster mappings for {}: {}", key, e);
        }
        info!("Deployment deleted successfully: {}", key);
        Ok(())
    }

    pub async fn get_cluster_mappings(
        &self,
        key: &str,
    ) -> Result<std::collections::HashMap<String, String>, PackageManagerError>
    {
        let map = self.storage.get_cluster_mappings(key).await?;
        Ok(map)
    }
}

// Shared math helpers (kept here to avoid circular deps)
#[allow(unused)]
pub fn compute_required_replicas(
    target: f64,
    p_single: f64,
    min_r: u32,
    max_r: u32,
) -> u32 {
    let eps = 1e-9;
    let t = target.clamp(0.0, 0.999999999);
    let p = p_single.clamp(eps, 0.999999999);
    if t <= p {
        return min_r.max(1);
    }
    let numerator = (1.0 - t).ln();
    let denom = (1.0 - p).ln();
    if denom.abs() < eps {
        return max_r;
    }
    let r = (numerator / denom).ceil() as u32;
    r.clamp(min_r.max(1), max_r.max(min_r))
}

pub fn cluster_quorum_availability(avails: &[f64]) -> f64 {
    let n = avails.len();
    if n == 0 {
        return 0.0;
    }
    let quorum = n / 2 + 1;
    let mut dist = vec![1.0f64];
    for &p in avails {
        let mut next = vec![0.0f64; dist.len() + 1];
        for k in 0..dist.len() {
            let base = dist[k];
            next[k] += base * (1.0 - p);
            next[k + 1] += base * p;
        }
        dist = next;
    }
    dist.into_iter()
        .enumerate()
        .filter_map(|(k, v)| if k >= quorum { Some(v) } else { None })
        .sum()
}

pub fn required_replicas_quorum(
    target: f64,
    sorted_worst_first: &[f64],
    max_r: u32,
) -> u32 {
    let t = target.clamp(0.0, 0.999999999);
    if sorted_worst_first.is_empty() {
        return 1;
    }
    let mut current: Vec<f64> = Vec::new();
    for (i, &p) in sorted_worst_first.iter().enumerate() {
        current.push(p.clamp(0.0, 1.0));
        let q_av = cluster_quorum_availability(&current);
        if q_av >= t {
            return (i + 1) as u32;
        }
        if (i as u32) + 1 >= max_r {
            break;
        }
    }
    (sorted_worst_first.len() as u32).min(max_r).max(1)
}

#[cfg(test)]
mod tests {
    use super::{
        cluster_quorum_availability, compute_required_replicas,
        required_replicas_quorum,
    };
    #[test]
    fn replicas_formula_basic() {
        assert_eq!(compute_required_replicas(0.99, 0.97, 1, 50), 2);
        assert_eq!(compute_required_replicas(0.9999, 0.97, 1, 50), 3);
        assert_eq!(compute_required_replicas(0.90, 0.97, 1, 50), 1);
        assert_eq!(compute_required_replicas(0.99, 0.5, 1, 50), 7);
    }
    #[test]
    fn quorum_availability_basic() {
        let av = [0.9, 0.9, 0.9];
        let q = cluster_quorum_availability(&av);
        assert!((q - 0.972).abs() < 1e-6);
    }
    #[test]
    fn strong_consistency_adjustment_2f_plus_1() {
        let mut av = vec![0.92, 0.94, 0.97];
        av.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let base = required_replicas_quorum(0.90, &av, 10);
        let strong_needed = 2 * base + 1;
        assert_eq!(base, 1);
        assert_eq!(strong_needed, 3);
    }
}
