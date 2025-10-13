use crate::traits::*;

#[cfg(feature = "memory")]
use crate::memory::{
    MemoryDeploymentStorage, MemoryPackageStorage, MemoryStorageFactory,
};

#[cfg(feature = "etcd")]
use crate::etcd::{
    EtcdConnectedFactory, EtcdCredentials, EtcdStorage, EtcdStorageFactory,
    EtcdTlsConfig,
};

/// A dynamic factory object that can create storage instances without exposing concrete types.
pub enum DynStorageFactory {
    #[cfg(feature = "memory")]
    Memory(MemoryStorageFactory),
    #[cfg(feature = "etcd")]
    Etcd(EtcdConnectedFactory),
}

// Concrete dynamic wrappers implementing the storage traits by delegation
pub enum DynPackageStorage {
    #[cfg(feature = "memory")]
    Memory(MemoryPackageStorage),
    #[cfg(feature = "etcd")]
    Etcd(EtcdStorage),
}

pub enum DynDeploymentStorage {
    #[cfg(feature = "memory")]
    Memory(MemoryDeploymentStorage),
    #[cfg(feature = "etcd")]
    Etcd(EtcdStorage),
}

#[async_trait::async_trait]
impl StorageHealth for DynPackageStorage {
    async fn health(&self) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynPackageStorage::Memory(s) => s.health().await,
            #[cfg(feature = "etcd")]
            DynPackageStorage::Etcd(s) => s.health().await,
        }
    }
}

#[async_trait::async_trait]
impl StorageHealth for DynDeploymentStorage {
    async fn health(&self) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => s.health().await,
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => s.health().await,
        }
    }
}

#[async_trait::async_trait]
impl PackageStorage for DynPackageStorage {
    async fn store_package(
        &self,
        package: &oprc_models::OPackage,
    ) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynPackageStorage::Memory(s) => s.store_package(package).await,
            #[cfg(feature = "etcd")]
            DynPackageStorage::Etcd(s) => s.store_package(package).await,
        }
    }

    async fn get_package(
        &self,
        name: &str,
    ) -> StorageResult<Option<oprc_models::OPackage>> {
        match self {
            #[cfg(feature = "memory")]
            DynPackageStorage::Memory(s) => s.get_package(name).await,
            #[cfg(feature = "etcd")]
            DynPackageStorage::Etcd(s) => s.get_package(name).await,
        }
    }

    async fn list_packages(
        &self,
        filter: crate::traits::PackageFilter,
    ) -> StorageResult<Vec<oprc_models::OPackage>> {
        match self {
            #[cfg(feature = "memory")]
            DynPackageStorage::Memory(s) => s.list_packages(filter).await,
            #[cfg(feature = "etcd")]
            DynPackageStorage::Etcd(s) => s.list_packages(filter).await,
        }
    }

    async fn delete_package(&self, name: &str) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynPackageStorage::Memory(s) => s.delete_package(name).await,
            #[cfg(feature = "etcd")]
            DynPackageStorage::Etcd(s) => s.delete_package(name).await,
        }
    }

    async fn package_exists(&self, name: &str) -> StorageResult<bool> {
        match self {
            #[cfg(feature = "memory")]
            DynPackageStorage::Memory(s) => s.package_exists(name).await,
            #[cfg(feature = "etcd")]
            DynPackageStorage::Etcd(s) => s.package_exists(name).await,
        }
    }
}

#[async_trait::async_trait]
impl DeploymentStorage for DynDeploymentStorage {
    async fn store_deployment(
        &self,
        deployment: &oprc_models::OClassDeployment,
    ) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => {
                s.store_deployment(deployment).await
            }
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => {
                s.store_deployment(deployment).await
            }
        }
    }

    async fn get_deployment(
        &self,
        key: &str,
    ) -> StorageResult<Option<oprc_models::OClassDeployment>> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => s.get_deployment(key).await,
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => s.get_deployment(key).await,
        }
    }

    async fn list_deployments(
        &self,
        filter: oprc_models::DeploymentFilter,
    ) -> StorageResult<Vec<oprc_models::OClassDeployment>> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => s.list_deployments(filter).await,
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => s.list_deployments(filter).await,
        }
    }

    async fn delete_deployment(&self, key: &str) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => s.delete_deployment(key).await,
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => s.delete_deployment(key).await,
        }
    }

    async fn deployment_exists(&self, key: &str) -> StorageResult<bool> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => s.deployment_exists(key).await,
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => s.deployment_exists(key).await,
        }
    }

    async fn save_cluster_mapping(
        &self,
        deployment_key: &str,
        cluster: &str,
        cluster_deployment_id: &str,
    ) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => {
                s.save_cluster_mapping(
                    deployment_key,
                    cluster,
                    cluster_deployment_id,
                )
                .await
            }
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => {
                s.save_cluster_mapping(
                    deployment_key,
                    cluster,
                    cluster_deployment_id,
                )
                .await
            }
        }
    }

    async fn get_cluster_mappings(
        &self,
        deployment_key: &str,
    ) -> StorageResult<std::collections::HashMap<String, String>> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => {
                s.get_cluster_mappings(deployment_key).await
            }
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => {
                s.get_cluster_mappings(deployment_key).await
            }
        }
    }

    async fn remove_cluster_mappings(
        &self,
        deployment_key: &str,
    ) -> StorageResult<()> {
        match self {
            #[cfg(feature = "memory")]
            DynDeploymentStorage::Memory(s) => {
                s.remove_cluster_mappings(deployment_key).await
            }
            #[cfg(feature = "etcd")]
            DynDeploymentStorage::Etcd(s) => {
                s.remove_cluster_mappings(deployment_key).await
            }
        }
    }
}

impl StorageFactory for DynStorageFactory {
    type PackageStorage = DynPackageStorage;
    type DeploymentStorage = DynDeploymentStorage;

    fn create_package_storage(&self) -> Self::PackageStorage {
        match self {
            #[cfg(feature = "memory")]
            DynStorageFactory::Memory(f) => {
                DynPackageStorage::Memory(f.create_package_storage())
            }
            #[cfg(feature = "etcd")]
            DynStorageFactory::Etcd(f) => {
                DynPackageStorage::Etcd(f.create_package_storage())
            }
        }
    }

    fn create_deployment_storage(&self) -> Self::DeploymentStorage {
        match self {
            #[cfg(feature = "memory")]
            DynStorageFactory::Memory(f) => {
                DynDeploymentStorage::Memory(f.create_deployment_storage())
            }
            #[cfg(feature = "etcd")]
            DynStorageFactory::Etcd(f) => {
                DynDeploymentStorage::Etcd(f.create_deployment_storage())
            }
        }
    }
}

#[cfg(feature = "etcd")]
pub struct EtcdFactoryConfig {
    pub endpoints: Vec<String>,
    pub key_prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub tls: Option<EtcdTlsConfig>,
}

#[cfg(feature = "etcd")]
impl EtcdFactoryConfig {
    pub async fn build(
        self,
    ) -> Result<DynStorageFactory, crate::error::StorageError> {
        let mut builder =
            EtcdStorageFactory::new(self.endpoints, self.key_prefix);
        if let (Some(u), Some(p)) = (self.username, self.password) {
            builder = builder.with_credentials(EtcdCredentials {
                username: u,
                password: p,
            });
        }
        if let Some(t) = self.timeout_seconds {
            builder = builder.with_timeout(t);
        }
        if let Some(tls) = self.tls {
            builder = builder.with_tls(tls);
        }
        let connected = builder.connect().await?;
        Ok(DynStorageFactory::Etcd(connected))
    }
}

#[cfg(feature = "memory")]
pub fn build_memory_factory() -> DynStorageFactory {
    DynStorageFactory::Memory(MemoryStorageFactory)
}
