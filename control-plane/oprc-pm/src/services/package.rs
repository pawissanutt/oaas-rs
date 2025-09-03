use crate::{
    config::DeploymentPolicyConfig,
    errors::{PackageError, PackageManagerError},
    models::{PackageFilter, PackageId},
    services::{DeploymentService, PackageValidator},
};
use oprc_cp_storage::PackageStorage;
use oprc_models::OPackage;
use std::sync::Arc;
use tracing::{info, warn};

pub struct PackageService {
    storage: Arc<dyn PackageStorage>,
    deployment_service: Arc<DeploymentService>,
    validator: PackageValidator,
    policy: DeploymentPolicyConfig,
}

impl PackageService {
    pub fn new(
        storage: Arc<dyn PackageStorage>,
        deployment_service: Arc<DeploymentService>,
        policy: DeploymentPolicyConfig,
    ) -> Self {
        Self {
            storage,
            deployment_service,
            validator: PackageValidator::new(),
            policy,
        }
    }

    pub async fn health(&self) -> Result<(), PackageManagerError> {
        self.storage.health().await.map_err(Into::into)
    }

    pub async fn create_package(
        &self,
        package: OPackage,
    ) -> Result<PackageId, PackageManagerError> {
        info!("Creating package: {}", package.name);

        // 1. Validate package structure
        // TODO: Add validation logic here
        // package.validate()?;
        self.validator.validate(&package).await?;

        // 2. Check if package already exists
        if self.storage.package_exists(&package.name).await? {
            return Err(PackageError::AlreadyExists(package.name).into());
        }

        // 3. Check dependencies
        self.validate_dependencies(&package.dependencies).await?;

        // 4. Store package
        self.storage.store_package(&package).await?;

        info!("Package created successfully: {}", package.name);

        // 5. Trigger deployments if configured
        // Note: This is typically handled separately through deployment API
        // but we could auto-deploy based on package metadata
        if let Err(e) =
            self.deployment_service.schedule_deployments(&package).await
        {
            warn!(
                "Failed to schedule deployments for package {}: {}",
                package.name, e
            );
            // Don't fail the package creation if deployment scheduling fails
        }

        Ok(PackageId::new(package.name))
    }

    pub async fn get_package(
        &self,
        name: &str,
    ) -> Result<Option<OPackage>, PackageManagerError> {
        info!("Getting package: {}", name);
        let package = self.storage.get_package(name).await?;
        Ok(package)
    }

    pub async fn list_packages(
        &self,
        filter: PackageFilter,
    ) -> Result<Vec<OPackage>, PackageManagerError> {
        info!("Listing packages with filter: {:?}", filter);

        // Convert our filter to the storage filter format
        let storage_filter = oprc_cp_storage::PackageFilter {
            name_pattern: filter.name_pattern,
            author: None, // Not implemented in our filter yet
            tags: filter.tags,
            disabled: filter.disabled,
        };

        let mut packages = self.storage.list_packages(storage_filter).await?;

        // Apply pagination
        if let Some(offset) = filter.offset {
            packages = packages.into_iter().skip(offset).collect();
        }

        if let Some(limit) = filter.limit {
            packages.truncate(limit);
        }

        Ok(packages)
    }

    pub async fn update_package(
        &self,
        package: OPackage,
    ) -> Result<(), PackageManagerError> {
        info!("Updating package: {}", package.name);

        // 1. Validate package structure
        // TODO: Add validation logic here
        // package.validate()?;
        self.validator.validate(&package).await?;

        // 2. Check if package exists
        if !self.storage.package_exists(&package.name).await? {
            return Err(PackageError::NotFound(package.name).into());
        }

        // 3. Validate dependencies
        self.validate_dependencies(&package.dependencies).await?;

        // 4. Store updated package
        self.storage.store_package(&package).await?;

        info!("Package updated successfully: {}", package.name);

        // TODO: Handle package updates with migration logic
        // This might involve:
        // - Checking for breaking changes
        // - Updating existing deployments
        // - Managing version compatibility

        Ok(())
    }

    pub async fn delete_package(
        &self,
        name: &str,
    ) -> Result<(), PackageManagerError> {
        info!("Deleting package: {}", name);

        // 1. Check if package exists
        if !self.storage.package_exists(name).await? {
            return Err(PackageError::NotFound(name.to_string()).into());
        }

        // TODO: Check for dependent packages
        // This requires implementing a dependency graph or reverse lookup

        // Optionally cascade delete active deployments referencing this package
        if self.policy.package_delete_cascade {
            let deployments = self
                .deployment_service
                .list_deployments(crate::models::DeploymentFilter {
                    package_name: Some(name.to_string()),
                    class_key: None,
                    target_env: None,
                    target_cluster: None,
                    limit: None,
                    offset: None,
                })
                .await?;
            for d in deployments {
                if let Err(e) =
                    self.deployment_service.delete_deployment(&d.key).await
                {
                    warn!(package=%name, deployment=%d.key, error=%e, "Failed cascading deployment delete");
                }
            }
        }

        // 3. Remove package
        self.storage.delete_package(name).await?;

        info!("Package deleted successfully: {}", name);
        Ok(())
    }

    async fn validate_dependencies(
        &self,
        dependencies: &[String],
    ) -> Result<(), PackageManagerError> {
        for dep in dependencies {
            if !self.storage.package_exists(dep).await? {
                return Err(
                    PackageError::DependencyNotFound(dep.clone()).into()
                );
            }
        }

        // TODO: Check for circular dependencies
        // This requires implementing a dependency graph traversal

        Ok(())
    }
}
