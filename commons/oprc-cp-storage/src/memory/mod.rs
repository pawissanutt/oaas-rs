use crate::traits::*;
use async_trait::async_trait;
use oprc_models::{DeploymentFilter, OClassDeployment, OPackage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type MemoryStore<T> = Arc<RwLock<HashMap<String, T>>>;

#[derive(Clone)]
pub struct MemoryPackageStorage {
    store: MemoryStore<OPackage>,
}

#[derive(Clone)]
pub struct MemoryDeploymentStorage {
    store: MemoryStore<OClassDeployment>,
    // mapping: deployment_key -> (cluster -> cluster_deployment_id)
    cluster_mappings: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
}

impl MemoryPackageStorage {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl MemoryDeploymentStorage {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            cluster_mappings: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl PackageStorage for MemoryPackageStorage {
    async fn store_package(&self, package: &OPackage) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.insert(package.name.clone(), package.clone());
        Ok(())
    }

    async fn get_package(&self, name: &str) -> StorageResult<Option<OPackage>> {
        let store = self.store.read().await;
        Ok(store.get(name).cloned())
    }

    async fn list_packages(
        &self,
        filter: PackageFilter,
    ) -> StorageResult<Vec<OPackage>> {
        let store = self.store.read().await;
        let packages: Vec<OPackage> = store
            .values()
            .filter(|package| {
                // Apply filters
                if let Some(ref pattern) = filter.name_pattern {
                    if !package.name.contains(pattern) {
                        return false;
                    }
                }

                if let Some(ref author) = filter.author {
                    if package.metadata.author.as_deref() != Some(author) {
                        return false;
                    }
                }

                if !filter.tags.is_empty() {
                    let has_matching_tag = filter
                        .tags
                        .iter()
                        .any(|tag| package.metadata.tags.contains(tag));
                    if !has_matching_tag {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        Ok(packages)
    }

    async fn delete_package(&self, name: &str) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.remove(name);
        Ok(())
    }

    async fn package_exists(&self, name: &str) -> StorageResult<bool> {
        let store = self.store.read().await;
        Ok(store.contains_key(name))
    }
}

#[async_trait]
impl StorageHealth for MemoryPackageStorage {
    async fn health(&self) -> StorageResult<()> {
        // In-memory backend is always healthy if process is alive
        Ok(())
    }
}

#[async_trait]
impl DeploymentStorage for MemoryDeploymentStorage {
    async fn store_deployment(
        &self,
        deployment: &OClassDeployment,
    ) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.insert(deployment.key.clone(), deployment.clone());
        Ok(())
    }

    async fn get_deployment(
        &self,
        key: &str,
    ) -> StorageResult<Option<OClassDeployment>> {
        let store = self.store.read().await;
        Ok(store.get(key).cloned())
    }

    async fn list_deployments(
        &self,
        filter: DeploymentFilter,
    ) -> StorageResult<Vec<OClassDeployment>> {
        let store = self.store.read().await;
        let deployments: Vec<OClassDeployment> = store
            .values()
            .filter(|deployment| {
                if let Some(ref package_name) = filter.package_name {
                    if deployment.package_name != *package_name {
                        return false;
                    }
                }

                if let Some(ref class_key) = filter.class_key {
                    if deployment.class_key != *class_key {
                        return false;
                    }
                }

                if let Some(ref target_env) = filter.target_env {
                    if !deployment.target_envs.contains(target_env) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        Ok(deployments)
    }

    async fn delete_deployment(&self, key: &str) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.remove(key);
        Ok(())
    }

    async fn deployment_exists(&self, key: &str) -> StorageResult<bool> {
        let store = self.store.read().await;
        Ok(store.contains_key(key))
    }

    async fn save_cluster_mapping(
        &self,
        deployment_key: &str,
        cluster: &str,
        cluster_deployment_id: &str,
    ) -> StorageResult<()> {
        let mut mappings = self.cluster_mappings.write().await;
        let entry = mappings
            .entry(deployment_key.to_string())
            .or_insert_with(HashMap::new);
        entry.insert(cluster.to_string(), cluster_deployment_id.to_string());
        Ok(())
    }

    async fn get_cluster_mappings(
        &self,
        deployment_key: &str,
    ) -> StorageResult<HashMap<String, String>> {
        let mappings = self.cluster_mappings.read().await;
        Ok(mappings.get(deployment_key).cloned().unwrap_or_default())
    }

    async fn remove_cluster_mappings(
        &self,
        deployment_key: &str,
    ) -> StorageResult<()> {
        let mut mappings = self.cluster_mappings.write().await;
        mappings.remove(deployment_key);
        Ok(())
    }
}

#[async_trait]
impl StorageHealth for MemoryDeploymentStorage {
    async fn health(&self) -> StorageResult<()> {
        Ok(())
    }
}

pub struct MemoryStorageFactory;

impl StorageFactory for MemoryStorageFactory {
    type PackageStorage = MemoryPackageStorage;
    type DeploymentStorage = MemoryDeploymentStorage;

    fn create_package_storage(&self) -> Self::PackageStorage {
        MemoryPackageStorage::new()
    }

    fn create_deployment_storage(&self) -> Self::DeploymentStorage {
        MemoryDeploymentStorage::new()
    }
}
