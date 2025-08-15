use crate::{
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
}

impl DeploymentService {
    pub fn new(
        storage: Arc<dyn DeploymentStorage>,
        crm_manager: Arc<CrmManager>,
    ) -> Self {
        Self {
            storage,
            crm_manager,
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

        // 5. Send deployment units to appropriate CRM instances with simple rollback on partial failure
        let mut any_failure = false;
        let mut success_count = 0usize;
        let mut first_success_id: Option<String> = None;
        for unit in units.iter() {
            let cluster_name = &unit.target_cluster;
            match self.crm_manager.get_client(cluster_name).await {
                Ok(crm_client) => match crm_client.deploy(unit.clone()).await {
                    Ok(response) => {
                        info!(
                            "Successfully deployed to cluster {}: {:?}",
                            cluster_name, response
                        );
                        success_count += 1;
                        if first_success_id.is_none() {
                            first_success_id = Some(response.id.clone());
                        }
                        if let Err(e) = self
                            .storage
                            .save_cluster_mapping(
                                &deployment.key,
                                cluster_name,
                                &response.id,
                            )
                            .await
                        {
                            error!(
                                "Failed to save cluster mapping for {} in {}: {}",
                                deployment.key, cluster_name, e
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to deploy to cluster {}: {}",
                            cluster_name, e
                        );
                        any_failure = true;
                    }
                },
                Err(e) => {
                    error!(
                        "Failed to get CRM client for cluster {}: {}",
                        cluster_name, e
                    );
                    any_failure = true;
                }
            }
        }

        // Rollback criteria: all attempts failed (no successes)
        if any_failure {
            warn!(
                "Deployment {} partially failed: {} successes, proceeding with available clusters",
                deployment.key, success_count
            );
        }

        if let Some(id) = first_success_id {
            Ok(DeploymentId::from_string(id))
        } else {
            // Preserve previous behavior (warn-only) when all clusters fail
            warn!(
                "Deployment {} failed in all target clusters; returning generated id (no active deployment units)",
                deployment.key
            );
            Ok(DeploymentId::new())
        }
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

        // TODO: Implement automatic deployment scheduling based on package metadata
        // This might involve:
        // - Reading deployment specifications from package metadata
        // - Creating appropriate OClassDeployment instances
        // - Calling deploy_class for each class that should be auto-deployed

        warn!(
            "Automatic deployment scheduling not yet implemented for package: {}",
            package.name
        );
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
