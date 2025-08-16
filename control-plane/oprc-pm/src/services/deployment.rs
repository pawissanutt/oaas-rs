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
use uuid::Uuid;

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

        // TODO: Implement proper requirement calculation based on:
        // - NFR requirements from deployment
        // - Resource requirements per function
        // - Availability requirements
        // - Performance requirements

        Ok(DeploymentRequirements {
            cpu_per_replica: 1.0,
            memory_per_replica: 512,
            min_replicas: 1,
            max_replicas: 10,
            target_replicas: 1, // TODO: Calculate based on NFR requirements
        })
    }

    async fn create_deployment_units(
        &self,
        class: &OClass,
        deployment: &OClassDeployment,
        _requirements: DeploymentRequirements,
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
                id: Uuid::new_v4().to_string(),
                package_name: deployment.package_name.clone(),
                class_key: deployment.class_key.clone(),
                target_cluster: cluster_name.clone(),
                functions: deployment
                    .functions
                    .iter()
                    .map(|f| {
                        let mut nf = f.clone();
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
                odgm: deployment.odgm.clone(),
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
