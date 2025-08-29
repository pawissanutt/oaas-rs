use crate::config::StorageConfig;
use anyhow::Result;
use oprc_cp_storage::unified::DynStorageFactory;

#[cfg(feature = "etcd")]
use oprc_cp_storage::etcd::EtcdTlsConfig as CpEtcdTlsConfig;
#[cfg(feature = "etcd")]
use oprc_cp_storage::unified::EtcdFactoryConfig;

pub async fn create_storage_factory(
    config: &StorageConfig,
) -> Result<DynStorageFactory> {
    match config.storage_type {
        crate::config::StorageType::Etcd => {
            #[cfg(feature = "etcd")]
            {
                let etcd = config
                    .etcd
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing etcd config"))?;
                let tls = etcd.tls.as_ref().map(|t| CpEtcdTlsConfig {
                    cert_file: t.client_cert.clone().unwrap_or_default(),
                    key_file: t.client_key.clone().unwrap_or_default(),
                    ca_file: t.ca_cert.clone(),
                });
                let cfg = EtcdFactoryConfig {
                    endpoints: etcd.endpoints.clone(),
                    key_prefix: etcd.key_prefix.clone(),
                    username: etcd.username.clone(),
                    password: etcd.password.clone(),
                    timeout_seconds: etcd.timeout,
                    tls,
                };
                return cfg.build().await.map_err(Into::into);
            }
            #[cfg(not(feature = "etcd"))]
            {
                bail!("oprc-pm built without etcd feature")
            }
        }
        crate::config::StorageType::Memory => {
            Ok(oprc_cp_storage::unified::build_memory_factory())
        }
    }
}
