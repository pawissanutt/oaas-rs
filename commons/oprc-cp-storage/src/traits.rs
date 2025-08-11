use async_trait::async_trait;
use oprc_models::{OPackage, OClassDeployment, RuntimeState, DeploymentFilter, RuntimeFilter};
use crate::error::StorageError;

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Clone)]
pub struct PackageFilter {
    pub name_pattern: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub disabled: Option<bool>,
}

#[async_trait]
pub trait PackageStorage: Send + Sync {
    async fn store_package(&self, package: &OPackage) -> StorageResult<()>;
    async fn get_package(&self, name: &str) -> StorageResult<Option<OPackage>>;
    async fn list_packages(&self, filter: PackageFilter) -> StorageResult<Vec<OPackage>>;
    async fn delete_package(&self, name: &str) -> StorageResult<()>;
    async fn package_exists(&self, name: &str) -> StorageResult<bool>;
}

#[async_trait]
pub trait DeploymentStorage: Send + Sync {
    async fn store_deployment(&self, deployment: &OClassDeployment) -> StorageResult<()>;
    async fn get_deployment(&self, key: &str) -> StorageResult<Option<OClassDeployment>>;
    async fn list_deployments(&self, filter: DeploymentFilter) -> StorageResult<Vec<OClassDeployment>>;
    async fn delete_deployment(&self, key: &str) -> StorageResult<()>;
    async fn deployment_exists(&self, key: &str) -> StorageResult<bool>;
}

#[async_trait]
pub trait RuntimeStorage: Send + Sync {
    async fn store_runtime_state(&self, state: &RuntimeState) -> StorageResult<()>;
    async fn get_runtime_state(&self, instance_id: &str) -> StorageResult<Option<RuntimeState>>;
    async fn list_runtime_states(&self, filter: RuntimeFilter) -> StorageResult<Vec<RuntimeState>>;
    async fn delete_runtime_state(&self, instance_id: &str) -> StorageResult<()>;
    async fn update_heartbeat(&self, instance_id: &str) -> StorageResult<()>;
}

pub trait StorageFactory {
    type PackageStorage: PackageStorage;
    type DeploymentStorage: DeploymentStorage;
    type RuntimeStorage: RuntimeStorage;
    
    fn create_package_storage(&self) -> Self::PackageStorage;
    fn create_deployment_storage(&self) -> Self::DeploymentStorage;
    fn create_runtime_storage(&self) -> Self::RuntimeStorage;
}
