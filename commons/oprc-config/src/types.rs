use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: Option<usize>,
    pub tls: Option<TlsConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            workers: None,
            tls: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
    pub cert_file: String,
    pub key_file: String,
    pub ca_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    pub log_level: String,
    pub json_format: bool,
    pub metrics_port: u16,
    pub jaeger_endpoint: Option<String>,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            json_format: true,
            metrics_port: 9090,
            jaeger_endpoint: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EtcdConfig {
    pub endpoints: Vec<String>,
    pub key_prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls: Option<TlsConfig>,
    pub timeout_seconds: Option<u64>,
}

impl Default for EtcdConfig {
    fn default() -> Self {
        Self {
            endpoints: vec!["http://localhost:2379".to_string()],
            key_prefix: "/oaas".to_string(),
            username: None,
            password: None,
            tls: None,
            timeout_seconds: Some(30),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    #[serde(rename = "type")]
    pub storage_type: StorageType,
    pub etcd: Option<EtcdConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageType {
    Memory,
    Etcd,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: StorageType::Memory,
            etcd: Some(EtcdConfig::default()),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceConfig {
    pub name: String,
    pub server: ServerConfig,
    pub observability: ObservabilityConfig,
    pub storage: StorageConfig,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name: "oaas-service".to_string(),
            server: ServerConfig::default(),
            observability: ObservabilityConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}
