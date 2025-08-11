use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use oprc_models::{OPackage, OClassDeployment, RuntimeState, DeploymentFilter, RuntimeFilter};
use crate::traits::*;

type MemoryStore<T> = Arc<RwLock<HashMap<String, T>>>;

#[derive(Clone)]
pub struct MemoryPackageStorage {
    store: MemoryStore<OPackage>,
}

#[derive(Clone)]
pub struct MemoryDeploymentStorage {
    store: MemoryStore<OClassDeployment>,
}

#[derive(Clone)]
pub struct MemoryRuntimeStorage {
    store: MemoryStore<RuntimeState>,
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
        }
    }
}

impl MemoryRuntimeStorage {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
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
    
    async fn list_packages(&self, filter: PackageFilter) -> StorageResult<Vec<OPackage>> {
        let store = self.store.read().await;
        let packages: Vec<OPackage> = store.values()
            .filter(|package| {
                // Apply filters
                if let Some(ref pattern) = filter.name_pattern {
                    if !package.name.contains(pattern) {
                        return false;
                    }
                }
                
                if let Some(ref author) = filter.author {
                    if package.metadata.author != *author {
                        return false;
                    }
                }
                
                if !filter.tags.is_empty() {
                    let has_matching_tag = filter.tags.iter()
                        .any(|tag| package.metadata.tags.contains(tag));
                    if !has_matching_tag {
                        return false;
                    }
                }
                
                if let Some(disabled) = filter.disabled {
                    if package.disabled != disabled {
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
impl DeploymentStorage for MemoryDeploymentStorage {
    async fn store_deployment(&self, deployment: &OClassDeployment) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.insert(deployment.key.clone(), deployment.clone());
        Ok(())
    }
    
    async fn get_deployment(&self, key: &str) -> StorageResult<Option<OClassDeployment>> {
        let store = self.store.read().await;
        Ok(store.get(key).cloned())
    }
    
    async fn list_deployments(&self, filter: DeploymentFilter) -> StorageResult<Vec<OClassDeployment>> {
        let store = self.store.read().await;
        let deployments: Vec<OClassDeployment> = store.values()
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
                    if deployment.target_env != *target_env {
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
}

#[async_trait]
impl RuntimeStorage for MemoryRuntimeStorage {
    async fn store_runtime_state(&self, state: &RuntimeState) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.insert(state.instance_id.clone(), state.clone());
        Ok(())
    }
    
    async fn get_runtime_state(&self, instance_id: &str) -> StorageResult<Option<RuntimeState>> {
        let store = self.store.read().await;
        Ok(store.get(instance_id).cloned())
    }
    
    async fn list_runtime_states(&self, filter: RuntimeFilter) -> StorageResult<Vec<RuntimeState>> {
        let store = self.store.read().await;
        let states: Vec<RuntimeState> = store.values()
            .filter(|state| {
                if let Some(ref deployment_id) = filter.deployment_id {
                    if state.deployment_id != *deployment_id {
                        return false;
                    }
                }
                
                if let Some(ref status) = filter.status {
                    if state.status != *status {
                        return false;
                    }
                }
                
                true
            })
            .cloned()
            .collect();
        
        Ok(states)
    }
    
    async fn delete_runtime_state(&self, instance_id: &str) -> StorageResult<()> {
        let mut store = self.store.write().await;
        store.remove(instance_id);
        Ok(())
    }
    
    async fn update_heartbeat(&self, instance_id: &str) -> StorageResult<()> {
        let mut store = self.store.write().await;
        if let Some(state) = store.get_mut(instance_id) {
            state.last_heartbeat = chrono::Utc::now();
        }
        Ok(())
    }
}

pub struct MemoryStorageFactory;

impl StorageFactory for MemoryStorageFactory {
    type PackageStorage = MemoryPackageStorage;
    type DeploymentStorage = MemoryDeploymentStorage;
    type RuntimeStorage = MemoryRuntimeStorage;
    
    fn create_package_storage(&self) -> Self::PackageStorage {
        MemoryPackageStorage::new()
    }
    
    fn create_deployment_storage(&self) -> Self::DeploymentStorage {
        MemoryDeploymentStorage::new()
    }
    
    fn create_runtime_storage(&self) -> Self::RuntimeStorage {
        MemoryRuntimeStorage::new()
    }
}
