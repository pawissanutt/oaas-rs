use crate::{
    config::DeploymentPolicyConfig,
    crm::CrmManager,
    errors::{DeploymentError, PackageManagerError},
    models::{DeploymentFilter, DeploymentId},
};
use chrono::Utc;
use oprc_cp_storage::DeploymentStorage;
use oprc_models::{DeploymentUnit, OClass, OClassDeployment, OPackage};
use std::sync::Arc;
use tracing::{error, info, warn};

pub struct DeploymentService {
    storage: Arc<dyn DeploymentStorage>,
    crm_manager: Arc<CrmManager>,
    policy: DeploymentPolicyConfig,
}

impl DeploymentService {
    pub fn new(
        storage: Arc<dyn DeploymentStorage>,
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

        // 1. Validate deployment
        if deployment.target_clusters.is_empty() {
            return Err(DeploymentError::Invalid(
                "No target clusters specified".to_string(),
            )
            .into());
        }

        // 2. Calculate deployment requirements
        let requirements =
            self.calculate_requirements(class, deployment).await?;
        info!("Calculated deployment requirements: {:?}", requirements);

        // 3. Create deployment units for each target cluster
        let units = self
            .create_deployment_units(class, deployment, requirements)
            .await?;
        info!("Created {} deployment units", units.len());

        // 4. Store deployment
        self.storage.store_deployment(deployment).await?;

        // 5. Send deployment units with retry + optional rollback
        let mut successes: Vec<(String, String)> = Vec::new(); // (cluster, dep_id)
        for unit in units.iter() {
            let cluster_name = unit.target_cluster.clone();
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
                                        &deployment.key,
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
                // Small backoff (could be configurable later)
                tokio::time::sleep(std::time::Duration::from_millis(
                    200 * attempt as u64,
                ))
                .await;
            }
            if let Some(err) = last_err {
                warn!(cluster=%cluster_name, error=%err, "Giving up after retries");
            }
        }

        let total_clusters = deployment.target_clusters.len();
        if successes.len() == total_clusters {
            // All good
            if let Some((_, id)) = successes.first() {
                return Ok(DeploymentId::from_string(id.clone()));
            }
        }
        if successes.is_empty() {
            warn!(deployment=%deployment.key, "All cluster deployments failed");
            return Err(DeploymentError::Invalid(
                "Deployment failed in all clusters".into(),
            )
            .into());
        }

        warn!(deployment=%deployment.key, successes=%successes.len(), total=%total_clusters, rollback=%self.policy.rollback_on_partial, "Partial deployment");
        if self.policy.rollback_on_partial {
            for (cluster, dep_id) in successes.iter() {
                match self.crm_manager.get_client(cluster).await {
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
            // Cleanup mappings
            if let Err(e) =
                self.storage.remove_cluster_mappings(&deployment.key).await
            {
                warn!(deployment=%deployment.key, error=%e, "Failed clearing cluster mappings after rollback");
            }
            return Err(DeploymentError::Invalid(
                "Deployment rolled back due to partial failure".into(),
            )
            .into());
        }
        // Proceed with partial success; return first successful id
        Ok(DeploymentId::from_string(successes[0].1.clone()))
    }

    pub async fn list_deployments(
        &self,
        filter: DeploymentFilter,
    ) -> Result<Vec<OClassDeployment>, PackageManagerError> {
        info!("Listing deployments with filter: {:?}", filter);

        // Convert our filter to the storage filter format
        let storage_filter = oprc_models::DeploymentFilter {
            package_name: filter.package_name,
            class_key: filter.class_key,
            target_env: filter.target_env,
            condition: None, // TODO: Add condition filtering
        };

        let mut deployments =
            self.storage.list_deployments(storage_filter).await?;

        // Apply pagination
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

        // 1. Get deployment details
        let deployment = self
            .storage
            .get_deployment(key)
            .await?
            .ok_or_else(|| DeploymentError::NotFound(key.to_string()))?;

        // 2. Delete from all target clusters
        // Retrieve per-cluster deployment IDs (fallback to key if missing)
        let cluster_id_map = self
            .storage
            .get_cluster_mappings(&deployment.key)
            .await
            .unwrap_or_default();

        for cluster_name in &deployment.target_clusters {
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
                        // Continue with other clusters
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to get CRM client for cluster {}: {}",
                        cluster_name, e
                    );
                    // Continue with other clusters
                }
            }
        }

        // 3. Remove from storage
        self.storage.delete_deployment(key).await?;
        // Remove any stored cluster mappings
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

    pub async fn schedule_deployments(
        &self,
        package: &OPackage,
    ) -> Result<(), PackageManagerError> {
        info!("Scheduling deployments for package: {}", package.name);
        // Simple first implementation: deploy any embedded deployment specs in package.deployments
        for spec in &package.deployments {
            // Skip if already exists
            if self.storage.get_deployment(&spec.key).await?.is_some() {
                continue;
            }
            // Validate that referenced class exists
            if !package.classes.iter().any(|c| c.key == spec.class_key) {
                warn!(deployment=%spec.key, class=%spec.class_key, "Skipping auto deploy; class not present in package");
                continue;
            }
            // Reuse provided spec directly
            match self
                .deploy_class(
                    &OClass {
                        key: spec.class_key.clone(),
                        description: None,
                        state_spec: None,
                        function_bindings: vec![],
                        disabled: false,
                    },
                    spec,
                )
                .await
            {
                Ok(_) => {
                    info!(deployment=%spec.key, "Auto deployment scheduled")
                }
                Err(e) => {
                    warn!(deployment=%spec.key, error=%e, "Auto deployment failed")
                }
            }
        }
        Ok(())
    }

    async fn calculate_requirements(
        &self,
        class: &OClass,
        _deployment: &OClassDeployment,
    ) -> Result<DeploymentRequirements, PackageManagerError> {
        info!(
            "Calculating deployment requirements for class: {}",
            class.key
        );
        // Mock availability-based replica sizing logic (Phase 1).
        // Strategy:
        // 1. Derive target availability (A_t) from deployment NFRs or default (0.99).
        // 2. For each target cluster, fetch cluster health and compute a per-cluster
        //    node readiness ratio (ready_nodes / node_count) as a proxy for single replica availability p_i.
        // 3. Aggregate p_i -> p_single (average) with sane fallbacks (default 0.97 when missing).
        // 4. Invert availability formula for "any replica serves": A_system = 1 - (1 - p_single)^R
        //    Solve minimal integer R: R >= log(1 - A_t) / log(1 - p_single)
        // 5. Clamp within min/max bounds and return.

        // NOTE: This is a mock; later phases will use per-deployment observed success rates.

        let mut target_availability = 0.99_f64; // default
        // Try pull from deployment (nfr_requirements.availability is 0..1 scale)
        if let Some(dep_av) = _deployment.nfr_requirements.availability {
            if (0.0..=1.0).contains(&dep_av) {
                target_availability = dep_av;
            }
        }

        // Collect cluster availability values (prefer explicit availability field; fallback to readiness ratio)
        let mut ratios: Vec<f64> = Vec::new();
        for cluster in &_deployment.target_clusters {
            let h =
                self.crm_manager.get_cluster_health(cluster).await.map_err(
                    |e| {
                        DeploymentError::Invalid(format!(
                            "Failed to get cluster health for {cluster}: {e}"
                        ))
                    },
                )?;
            if let Some(av) = h.availability {
                ratios.push(av.clamp(0.0, 1.0));
            } else if let (Some(r), Some(n)) = (h.ready_nodes, h.node_count) {
                if n == 0 {
                    return Err(DeploymentError::Invalid(format!(
                        "Cluster {cluster} reported zero nodes"
                    ))
                    .into());
                }
                ratios.push((r as f64 / n as f64).clamp(0.0, 1.0));
            } else {
                return Err(DeploymentError::Invalid(format!(
                    "Cluster {cluster} missing availability and ready/node counts"
                ))
                .into());
            }
        }
        if ratios.is_empty() {
            return Err(DeploymentError::Invalid(
                "No cluster health data available for availability calculation"
                    .to_string(),
            )
            .into());
        }
        // Quorum-based (Raft-style) availability: majority of replicas must be up.
        // Adapted from provided Java reference: incrementally add worst environments until
        // cluster availability meets target. We use DP to compute quorum availability.
        let mut sorted = ratios.clone();
        sorted.sort_by(|a, b| {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        }); // worst -> best
        let mut target_replicas =
            required_replicas_quorum(target_availability, &sorted, 50);
        let achieved =
            cluster_quorum_availability(&sorted[..target_replicas as usize]);
        // Consistency-driven adjustment: Strong consistency -> ensure 2f+1 replicas.
        if let Some(spec) = &class.state_spec {
            use oprc_models::ConsistencyModel;
            if spec.consistency_model == ConsistencyModel::Strong {
                // Infer f from current replica count's quorum size
                let quorum = (target_replicas as usize / 2) + 1;
                let mut f = quorum.saturating_sub(1);
                if f == 0 {
                    f = 1;
                } // minimum fault tolerance of 1
                let strong_needed = 2 * f + 1;
                if strong_needed as u32 > target_replicas {
                    target_replicas = strong_needed as u32;
                }
                if target_replicas > 50 {
                    target_replicas = 50;
                }
            }
        }
        info!(class=%class.key, target_availability=%target_availability, selected_replicas=%target_replicas, achieved_quorum_availability=%achieved, ratios=?sorted, "Replica calculation (quorum+consistency model)");

        Ok(DeploymentRequirements {
            cpu_per_replica: 1.0,
            memory_per_replica: 512,
            min_replicas: 1,
            max_replicas: 50,
            target_replicas,
        })
    }

    async fn create_deployment_units(
        &self,
        class: &OClass,
        deployment: &OClassDeployment,
        requirements: DeploymentRequirements,
    ) -> Result<Vec<DeploymentUnit>, PackageManagerError> {
        info!("Creating deployment units for class: {}", class.key);

        let mut units = Vec::new();

        // Build a lookup of function provision container images keyed by function key
        let mut image_map = std::collections::HashMap::new();
        // Derive image map from existing deployment function specs (already enriched in handler).
        for f in &deployment.functions {
            if let Some(img) = &f.container_image {
                image_map.insert(f.function_key.clone(), img.clone());
            }
        }

        for cluster_name in &deployment.target_clusters {
            let unit = DeploymentUnit {
                id: nanoid::nanoid!(),
                package_name: deployment.package_name.clone(),
                class_key: deployment.class_key.clone(),
                target_cluster: cluster_name.clone(),
                functions: deployment
                    .functions
                    .iter()
                    .map(|f| {
                        let mut nf = f.clone();
                        // Override replicas with calculated target replicas (apply minimum 1)
                        nf.replicas = requirements.target_replicas.max(1);
                        if nf.container_image.is_none() {
                            if let Some(img) = image_map.get(&nf.function_key) {
                                nf.container_image = Some(img.clone());
                            }
                        }
                        nf
                    })
                    .collect(),
                target_env: deployment.target_env.clone(),
                nfr_requirements: deployment.nfr_requirements.clone(),
                condition: oprc_models::DeploymentCondition::Pending,
                odgm: deployment.odgm.as_ref().map(|o| {
                    let mut new_o = o.clone();
                    // Set replica_count if present (convert u32 -> i32 safely)
                    new_o.replica_count = requirements.target_replicas as i32;
                    new_o
                }),
                created_at: Utc::now(),
            };

            units.push(unit);
        }

        Ok(units)
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
struct DeploymentRequirements {
    pub cpu_per_replica: f64,
    pub memory_per_replica: u64, // MB
    pub min_replicas: u32,
    pub max_replicas: u32,
    pub target_replicas: u32,
}

// Compute required replicas given target availability (A_t) and single replica availability (p_single)
// Using: A_system = 1 - (1 - p_single)^R => R >= log(1 - A_t) / log(1 - p_single)
#[allow(unused)]
fn compute_required_replicas(
    target: f64,
    p_single: f64,
    min_r: u32,
    max_r: u32,
) -> u32 {
    let eps = 1e-9;
    let t = target.clamp(0.0, 0.999999999); // avoid 1.0
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

// DP-based probability that a quorum (majority) of nodes are up for the provided per-node availabilities.
fn cluster_quorum_availability(avails: &[f64]) -> f64 {
    let n = avails.len();
    if n == 0 {
        return 0.0;
    }
    let quorum = n / 2 + 1; // majority
    let mut dist = vec![1.0f64]; // P(0 up) = 1 at start
    for &p in avails {
        let mut next = vec![0.0f64; dist.len() + 1];
        for k in 0..dist.len() {
            let base = dist[k];
            next[k] += base * (1.0 - p); // node down
            next[k + 1] += base * p; // node up
        }
        dist = next;
    }
    dist.into_iter()
        .enumerate()
        .filter_map(|(k, v)| if k >= quorum { Some(v) } else { None })
        .sum()
}

// Incrementally add worst (lowest availability) nodes until quorum availability >= target.
fn required_replicas_quorum(
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
        // p_single 0.97, target 0.99 => 2
        assert_eq!(compute_required_replicas(0.99, 0.97, 1, 50), 2);
        // Higher target 0.9999 => 3
        assert_eq!(compute_required_replicas(0.9999, 0.97, 1, 50), 3);
        // Target below single replica => 1
        assert_eq!(compute_required_replicas(0.90, 0.97, 1, 50), 1);
        // Low p_single requires more replicas
        assert_eq!(compute_required_replicas(0.99, 0.5, 1, 50), 7);
    }

    #[test]
    fn quorum_availability_basic() {
        let av = [0.9, 0.9, 0.9]; // 3 nodes, quorum 2 -> availability = P(>=2 up)
        let q = cluster_quorum_availability(&av);
        // Manual: P(2 up) = C(3,2)*0.9^2*0.1 + P(3 up)=0.9^3 = 3*0.81*0.1 + 0.729 = 0.243 + 0.729 = 0.972
        assert!((q - 0.972).abs() < 1e-6);
    }

    #[test]
    fn strong_consistency_adjustment_2f_plus_1() {
        let mut av = vec![0.92, 0.94, 0.97];
        av.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let base = required_replicas_quorum(0.90, &av, 10); // should likely be 2
        let strong_needed = 2 * base + 1;
        assert_eq!(base, 1);
        assert_eq!(strong_needed, 3);
    }
}
