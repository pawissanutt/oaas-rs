use crate::{
    config::DeploymentPolicyConfig,
    errors::{PackageError, PackageManagerError},
    models::{PackageFilter, PackageId},
    services::{DeploymentService, PackageValidator},
};
use oprc_cp_storage::PackageStorage;
use oprc_models::OPackage;
use std::sync::Arc;
use tracing::{debug, info, warn};

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
        let pkg_name = package.name.clone();

        // 1. Validate package structure
        // TODO: Add validation logic here
        // package.validate()?;
        self.validator.validate(&package).await?;

        // 2. Check if package already exists
        if self.storage.package_exists(&pkg_name).await? {
            return Err(PackageError::AlreadyExists(pkg_name.clone()).into());
        }

        // 3. Check dependencies
        self.validate_dependencies(&package.dependencies).await?;

        // 4. Store package (strip deployments; managed by deployment service)
        let mut store_pkg = package;
        if !store_pkg.deployments.is_empty() {
            debug!(
                package=%pkg_name,
                count=%store_pkg.deployments.len(),
                "Dropping embedded deployments from package before storing"
            );
            store_pkg.deployments.clear();
        }
        self.storage.store_package(&store_pkg).await?;

        info!("Package created successfully: {}", pkg_name);

        Ok(PackageId::new(pkg_name))
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
        let pkg_name = package.name.clone();

        // 1. Validate package structure
        // TODO: Add validation logic here
        // package.validate()?;
        self.validator.validate(&package).await?;

        // 2. Check if package exists
        if !self.storage.package_exists(&pkg_name).await? {
            return Err(PackageError::NotFound(pkg_name.clone()).into());
        }

        // 3. Validate dependencies
        self.validate_dependencies(&package.dependencies).await?;

        // 4. Store updated package (strip deployments; managed by deployment service)
        let mut store_pkg = package;
        if !store_pkg.deployments.is_empty() {
            warn!(
                package=%pkg_name,
                count=%store_pkg.deployments.len(),
                "Dropping embedded deployments from package before storing"
            );
            store_pkg.deployments.clear();
        }
        self.storage.store_package(&store_pkg).await?;

        info!("Package updated successfully: {}", pkg_name);

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
