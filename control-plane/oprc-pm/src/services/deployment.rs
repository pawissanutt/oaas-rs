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

        // 5. Send deployment units to appropriate CRM instances
        for unit in units {
            let cluster_name = &unit.target_cluster;
            match self.crm_manager.get_client(cluster_name).await {
                Ok(crm_client) => {
                    match crm_client.deploy(unit.clone()).await {
                        Ok(response) => {
                            info!(
                                "Successfully deployed to cluster {}: {:?}",
                                cluster_name, response
                            );
                        }
                        Err(e) => {
                            error!(
                                "Failed to deploy to cluster {}: {}",
                                cluster_name, e
                            );
                            // TODO: Implement rollback or retry logic
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to get CRM client for cluster {}: {}",
                        cluster_name, e
                    );
                    return Err(DeploymentError::ClusterUnavailable(
                        cluster_name.clone(),
                    )
                    .into());
                }
            }
        }

        Ok(DeploymentId::new())
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
        for cluster_name in &deployment.target_clusters {
            match self.crm_manager.get_client(cluster_name).await {
                Ok(crm_client) => {
                    // TODO: Need to track deployment IDs per cluster
                    // For now, we'll use the deployment key
                    if let Err(e) = crm_client.delete_deployment(key).await {
                        warn!(
                            "Failed to delete deployment from cluster {}: {}",
                            cluster_name, e
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

        info!("Deployment deleted successfully: {}", key);
        Ok(())
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

        for cluster_name in &deployment.target_clusters {
            let unit = DeploymentUnit {
                id: Uuid::new_v4().to_string(),
                package_name: deployment.package_name.clone(),
                class_key: deployment.class_key.clone(),
                target_cluster: cluster_name.clone(),
                functions: deployment.functions.clone(),
                target_env: deployment.target_env.clone(),
                nfr_requirements: deployment.nfr_requirements.clone(),
                condition: oprc_models::DeploymentCondition::Pending,
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
