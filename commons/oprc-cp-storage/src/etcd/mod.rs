use crate::error::StorageError;
use crate::traits::*;
use async_trait::async_trait;
use etcd_rs::{Client, ClientConfig, GetOptions};
use oprc_models::{
    DeploymentFilter, OClassDeployment, OPackage, RuntimeFilter, RuntimeState,
};

#[derive(Clone)]
pub struct EtcdCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Clone)]
pub struct EtcdTlsConfig {
    pub cert_file: String,
    pub key_file: String,
    pub ca_file: Option<String>,
}

pub struct EtcdStorageFactory {
    endpoints: Vec<String>,
    key_prefix: String,
    credentials: Option<EtcdCredentials>,
    tls_config: Option<EtcdTlsConfig>,
    timeout_seconds: Option<u64>,
}

impl EtcdStorageFactory {
    pub fn new(endpoints: Vec<String>, key_prefix: String) -> Self {
        Self {
            endpoints,
            key_prefix,
            credentials: None,
            tls_config: None,
            timeout_seconds: None,
        }
    }

    pub fn with_credentials(mut self, credentials: EtcdCredentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    pub fn with_tls(mut self, tls_config: EtcdTlsConfig) -> Self {
        self.tls_config = Some(tls_config);
        self
    }

    pub fn with_timeout(mut self, timeout_seconds: u64) -> Self {
        self.timeout_seconds = Some(timeout_seconds);
        self
    }

    async fn create_client(&self) -> Result<Client, StorageError> {
        let mut config = ClientConfig::new(self.endpoints.clone());

        if let Some(ref creds) = self.credentials {
            config = config
                .with_auth(creds.username.clone(), creds.password.clone());
        }

        if let Some(timeout) = self.timeout_seconds {
            config =
                config.with_timeout(std::time::Duration::from_secs(timeout));
        }

        Client::connect(config)
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))
    }
}

#[derive(Clone)]
pub struct EtcdStorage {
    client: Client,
    key_prefix: String,
}

impl EtcdStorage {
    pub async fn new(
        factory: &EtcdStorageFactory,
    ) -> Result<Self, StorageError> {
        let client = factory.create_client().await?;
        Ok(Self {
            client,
            key_prefix: factory.key_prefix.clone(),
        })
    }

    fn package_key(&self, name: &str) -> String {
        format!("{}/packages/{}", self.key_prefix, name)
    }

    fn deployment_key(&self, key: &str) -> String {
        format!("{}/deployments/{}", self.key_prefix, key)
    }

    fn runtime_key(&self, instance_id: &str) -> String {
        format!("{}/runtime/{}", self.key_prefix, instance_id)
    }
}

#[async_trait]
impl PackageStorage for EtcdStorage {
    async fn store_package(&self, package: &OPackage) -> StorageResult<()> {
        let key = self.package_key(&package.name);
        let value = serde_json::to_string(package)?;

        self.client.put(key, value, None).await?;
        Ok(())
    }

    async fn get_package(&self, name: &str) -> StorageResult<Option<OPackage>> {
        let key = self.package_key(name);

        match self.client.get(key, None).await {
            Ok(response) => {
                if let Some(kv) = response.kvs().first() {
                    let package: OPackage = serde_json::from_slice(kv.value())?;
                    Ok(Some(package))
                } else {
                    Ok(None)
                }
            }
            Err(etcd_rs::Error::GRpcStatus(status))
                if status.code() == tonic::Code::NotFound =>
            {
                Ok(None)
            }
            Err(e) => Err(StorageError::Etcd(e)),
        }
    }

    async fn list_packages(
        &self,
        _filter: PackageFilter,
    ) -> StorageResult<Vec<OPackage>> {
        let prefix = format!("{}/packages/", self.key_prefix);
        let options = GetOptions::new().with_prefix();

        let response = self.client.get(prefix, Some(options)).await?;
        let mut packages = Vec::new();

        for kv in response.kvs() {
            let package: OPackage = serde_json::from_slice(kv.value())?;
            packages.push(package);
        }

        Ok(packages)
    }

    async fn delete_package(&self, name: &str) -> StorageResult<()> {
        let key = self.package_key(name);
        self.client.delete(key, None).await?;
        Ok(())
    }

    async fn package_exists(&self, name: &str) -> StorageResult<bool> {
        let key = self.package_key(name);

        match self.client.get(key, None).await {
            Ok(response) => Ok(!response.kvs().is_empty()),
            Err(etcd_rs::Error::GRpcStatus(status))
                if status.code() == tonic::Code::NotFound =>
            {
                Ok(false)
            }
            Err(e) => Err(StorageError::Etcd(e)),
        }
    }
}

impl StorageFactory for EtcdStorageFactory {
    type PackageStorage = EtcdStorage;
    type DeploymentStorage = EtcdStorage;
    type RuntimeStorage = EtcdStorage;

    fn create_package_storage(&self) -> Self::PackageStorage {
        // Note: This is a simplified implementation
        // In practice, you'd need to handle the async creation differently
        todo!("Implement async factory pattern")
    }

    fn create_deployment_storage(&self) -> Self::DeploymentStorage {
        todo!("Implement async factory pattern")
    }

    fn create_runtime_storage(&self) -> Self::RuntimeStorage {
        todo!("Implement async factory pattern")
    }
}
