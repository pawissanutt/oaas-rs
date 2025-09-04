use crate::error::StorageError;
use crate::traits::*;
use async_trait::async_trait;
use etcd_client::{Client, ConnectOptions, DeleteOptions, GetOptions};
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

/// A configuration builder for connecting to etcd. Call `connect()` to obtain a connected factory.
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

    /// Establishes the etcd connection and returns a connected factory capable of
    /// synchronously creating storage instances.
    pub async fn connect(&self) -> Result<EtcdConnectedFactory, StorageError> {
        let mut opts = ConnectOptions::new();

        if let Some(ref creds) = self.credentials {
            opts =
                opts.with_user(creds.username.clone(), creds.password.clone());
        }

        if let Some(timeout) = self.timeout_seconds {
            opts = opts.with_timeout(std::time::Duration::from_secs(timeout));
        }

        // Note: TLS config is not wired here yet; requires HTTPS endpoints and TlsOptions
        // If needed, extend EtcdTlsConfig to provide PEM contents and use TlsOptions
        let client = Client::connect(self.endpoints.clone(), Some(opts))
            .await
            .map_err(|e| StorageError::Connection(e.to_string()))?;

        Ok(EtcdConnectedFactory {
            client,
            key_prefix: self.key_prefix.clone(),
        })
    }
}

/// A connected etcd factory holding a ready client. Can create storage instances without async.
#[derive(Clone)]
pub struct EtcdConnectedFactory {
    pub(crate) client: Client,
    pub(crate) key_prefix: String,
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
        // Backwards convenience: build a connected factory and create from it.
        let connected = factory.connect().await?;
        Ok(Self::from_parts(connected.client, connected.key_prefix))
    }

    /// Create a storage from an existing etcd client and key prefix.
    pub fn from_parts(client: Client, key_prefix: String) -> Self {
        Self { client, key_prefix }
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

    fn cluster_mapping_prefix(&self, deployment_key: &str) -> String {
        format!(
            "{}/deployments/{}/clusters/",
            self.key_prefix, deployment_key
        )
    }
}

#[async_trait]
impl PackageStorage for EtcdStorage {
    async fn store_package(&self, package: &OPackage) -> StorageResult<()> {
        let key = self.package_key(&package.name);
        let value = serde_json::to_string(package)?;

        let mut client = self.client.clone();
        client.put(key, value, None).await.map(|_| ())?;
        Ok(())
    }

    async fn get_package(&self, name: &str) -> StorageResult<Option<OPackage>> {
        let key = self.package_key(name);

        let mut client = self.client.clone();
        match client.get(key, None).await {
            Ok(response) => {
                if let Some(kv) = response.kvs().first() {
                    let package: OPackage = serde_json::from_slice(kv.value())?;
                    Ok(Some(package))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                // etcd-client returns Ok with empty kvs for not found; treat any error as backend
                Err(StorageError::Backend(e.to_string()))
            }
        }
    }

    async fn list_packages(
        &self,
        _filter: PackageFilter,
    ) -> StorageResult<Vec<OPackage>> {
        let prefix = format!("{}/packages/", self.key_prefix);
        let options = GetOptions::new().with_prefix();

        let mut client = self.client.clone();
        let response = client
            .get(prefix, Some(options))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut packages = Vec::new();

        for kv in response.kvs() {
            let package: OPackage = serde_json::from_slice(kv.value())?;
            packages.push(package);
        }

        Ok(packages)
    }

    async fn delete_package(&self, name: &str) -> StorageResult<()> {
        let key = self.package_key(name);
        let mut client = self.client.clone();
        client
            .delete(key, None)
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn package_exists(&self, name: &str) -> StorageResult<bool> {
        let key = self.package_key(name);

        let mut client = self.client.clone();
        match client.get(key, None).await {
            Ok(response) => Ok(!response.kvs().is_empty()),
            Err(e) => Err(StorageError::Backend(e.to_string())),
        }
    }
}

#[async_trait]
impl StorageHealth for EtcdStorage {
    async fn health(&self) -> StorageResult<()> {
        // Perform a cheap prefixed get with a very small timeout via etcd client options.
        // We clone the client to avoid mutability issues.
        let mut client = self.client.clone();
        let key = format!("{}/health", self.key_prefix);
        let res = client.get(key, None).await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => Err(StorageError::Backend(e.to_string())),
        }
    }
}

#[async_trait]
impl DeploymentStorage for EtcdStorage {
    async fn store_deployment(
        &self,
        deployment: &OClassDeployment,
    ) -> StorageResult<()> {
        let key = self.deployment_key(&deployment.key);
        let value = serde_json::to_string(deployment)?;
        let mut client = self.client.clone();
        client
            .put(key, value, None)
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get_deployment(
        &self,
        key: &str,
    ) -> StorageResult<Option<OClassDeployment>> {
        let key = self.deployment_key(key);
        let mut client = self.client.clone();
        let resp = client
            .get(key, None)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        if let Some(kv) = resp.kvs().first() {
            let deployment: OClassDeployment =
                serde_json::from_slice(kv.value())?;
            Ok(Some(deployment))
        } else {
            Ok(None)
        }
    }

    async fn list_deployments(
        &self,
        filter: DeploymentFilter,
    ) -> StorageResult<Vec<OClassDeployment>> {
        let prefix = format!("{}/deployments/", self.key_prefix);
        let options = GetOptions::new().with_prefix();
        let mut client = self.client.clone();
        let resp = client
            .get(prefix, Some(options))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut deployments: Vec<OClassDeployment> = Vec::new();
        for kv in resp.kvs() {
            // Only consider top-level deployment records at: {prefix}/deployments/{key}
            // Skip nested entries such as cluster mappings stored under:
            // {prefix}/deployments/{key}/clusters/{cluster}
            let full_key = String::from_utf8_lossy(kv.key());
            let base = format!("{}/deployments/", self.key_prefix);
            if let Some(rest) = full_key.strip_prefix(&base) {
                if rest.contains('/') {
                    // Nested path -> not a deployment record
                    continue;
                }
            }

            // Try to parse the value; skip entries that aren't valid deployment JSON
            let d: OClassDeployment = match serde_json::from_slice(kv.value()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Apply filters similar to memory backend
            if let Some(ref package_name) = filter.package_name {
                if &d.package_name != package_name {
                    continue;
                }
            }
            if let Some(ref class_key) = filter.class_key {
                if &d.class_key != class_key {
                    continue;
                }
            }
            if let Some(ref target_env) = filter.target_env {
                if !d.target_envs.contains(target_env) {
                    continue;
                }
            }
            deployments.push(d);
        }
        Ok(deployments)
    }

    async fn delete_deployment(&self, key: &str) -> StorageResult<()> {
        let key = self.deployment_key(key);
        let mut client = self.client.clone();
        client
            .delete(key, None)
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn deployment_exists(&self, key: &str) -> StorageResult<bool> {
        let key = self.deployment_key(key);
        let options = None;
        let mut client = self.client.clone();
        let resp = client
            .get(key, options)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(!resp.kvs().is_empty())
    }

    async fn save_cluster_mapping(
        &self,
        deployment_key: &str,
        cluster: &str,
        cluster_deployment_id: &str,
    ) -> StorageResult<()> {
        let key = format!(
            "{}{}",
            self.cluster_mapping_prefix(deployment_key),
            cluster
        );
        let mut client = self.client.clone();
        client
            .put(key, cluster_deployment_id.to_string(), None)
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get_cluster_mappings(
        &self,
        deployment_key: &str,
    ) -> StorageResult<std::collections::HashMap<String, String>> {
        let prefix = self.cluster_mapping_prefix(deployment_key);
        let options = GetOptions::new().with_prefix();
        let mut client = self.client.clone();
        let resp = client
            .get(prefix, Some(options))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut map = std::collections::HashMap::new();
        for kv in resp.kvs() {
            // key is bytes; convert to string and strip the prefix
            let full_key = String::from_utf8_lossy(kv.key());
            let cluster = full_key
                .strip_prefix(&self.cluster_mapping_prefix(deployment_key))
                .unwrap_or(&full_key)
                .to_string();
            map.insert(
                cluster,
                String::from_utf8_lossy(kv.value()).to_string(),
            );
        }
        Ok(map)
    }

    async fn remove_cluster_mappings(
        &self,
        deployment_key: &str,
    ) -> StorageResult<()> {
        let prefix = self.cluster_mapping_prefix(deployment_key);
        let options = DeleteOptions::new().with_prefix();
        let mut client = self.client.clone();
        client
            .delete(prefix, Some(options))
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))
    }
}

#[async_trait]
impl RuntimeStorage for EtcdStorage {
    async fn store_runtime_state(
        &self,
        state: &RuntimeState,
    ) -> StorageResult<()> {
        let key = self.runtime_key(&state.instance_id);
        let value = serde_json::to_string(state)?;
        let mut client = self.client.clone();
        client
            .put(key, value, None)
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get_runtime_state(
        &self,
        instance_id: &str,
    ) -> StorageResult<Option<RuntimeState>> {
        let key = self.runtime_key(instance_id);
        let mut client = self.client.clone();
        let resp = client
            .get(key, None)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        if let Some(kv) = resp.kvs().first() {
            let state: RuntimeState = serde_json::from_slice(kv.value())?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    async fn list_runtime_states(
        &self,
        filter: RuntimeFilter,
    ) -> StorageResult<Vec<RuntimeState>> {
        let prefix = format!("{}/runtime/", self.key_prefix);
        let options = GetOptions::new().with_prefix();
        let mut client = self.client.clone();
        let resp = client
            .get(prefix, Some(options))
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        let mut states: Vec<RuntimeState> = Vec::new();
        for kv in resp.kvs() {
            let s: RuntimeState = serde_json::from_slice(kv.value())?;
            if let Some(ref deployment_id) = filter.deployment_id {
                if &s.deployment_id != deployment_id {
                    continue;
                }
            }
            if let Some(ref status) = filter.status {
                if &s.status != status {
                    continue;
                }
            }
            states.push(s);
        }
        Ok(states)
    }

    async fn delete_runtime_state(
        &self,
        instance_id: &str,
    ) -> StorageResult<()> {
        let key = self.runtime_key(instance_id);
        let mut client = self.client.clone();
        client
            .delete(key, None)
            .await
            .map(|_| ())
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn update_heartbeat(&self, instance_id: &str) -> StorageResult<()> {
        // Read-modify-write the state
        let mut state = match self.get_runtime_state(instance_id).await? {
            Some(s) => s,
            None => {
                return Err(StorageError::NotFound(instance_id.to_string()));
            }
        };
        state.last_heartbeat = chrono::Utc::now();
        self.store_runtime_state(&state).await
    }
}
// Note: The connected factory can synchronously create storages sharing the same client.
impl StorageFactory for EtcdConnectedFactory {
    type PackageStorage = EtcdStorage;
    type DeploymentStorage = EtcdStorage;
    type RuntimeStorage = EtcdStorage;

    fn create_package_storage(&self) -> Self::PackageStorage {
        EtcdStorage::from_parts(self.client.clone(), self.key_prefix.clone())
    }

    fn create_deployment_storage(&self) -> Self::DeploymentStorage {
        EtcdStorage::from_parts(self.client.clone(), self.key_prefix.clone())
    }

    fn create_runtime_storage(&self) -> Self::RuntimeStorage {
        EtcdStorage::from_parts(self.client.clone(), self.key_prefix.clone())
    }
}
