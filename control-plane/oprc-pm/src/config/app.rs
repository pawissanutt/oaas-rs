use anyhow::Result;
use envconfig::Envconfig;
use std::collections::HashMap;

#[derive(Debug, Clone, Envconfig)]
pub struct AppConfig {
    // Server configuration
    #[envconfig(from = "SERVER_HOST", default = "0.0.0.0")]
    pub server_host: String,

    #[envconfig(from = "SERVER_PORT", default = "8080")]
    pub server_port: u16,

    #[envconfig(from = "SERVER_WORKERS")]
    pub server_workers: Option<usize>,

    // Storage configuration
    #[envconfig(from = "STORAGE_TYPE", default = "memory")]
    pub storage_type: String,

    #[envconfig(from = "ETCD_ENDPOINTS", default = "localhost:2379")]
    pub etcd_endpoints: String,

    #[envconfig(from = "ETCD_KEY_PREFIX", default = "/oaas/pm")]
    pub etcd_key_prefix: String,

    #[envconfig(from = "ETCD_USERNAME")]
    pub etcd_username: Option<String>,

    #[envconfig(from = "ETCD_PASSWORD")]
    pub etcd_password: Option<String>,

    #[envconfig(from = "ETCD_TIMEOUT", default = "30")]
    pub etcd_timeout_seconds: u64,

    #[envconfig(from = "ETCD_TLS_ENABLED", default = "false")]
    pub etcd_tls_enabled: bool,

    #[envconfig(from = "ETCD_CA_CERT_PATH")]
    pub etcd_ca_cert_path: Option<String>,

    #[envconfig(from = "ETCD_CLIENT_CERT_PATH")]
    pub etcd_client_cert_path: Option<String>,

    #[envconfig(from = "ETCD_CLIENT_KEY_PATH")]
    pub etcd_client_key_path: Option<String>,

    #[envconfig(from = "ETCD_TLS_INSECURE", default = "false")]
    pub etcd_tls_insecure: bool,

    // CRM configuration
    #[envconfig(from = "CRM_DEFAULT_URL")]
    pub crm_default_url: Option<String>,

    #[envconfig(from = "CRM_DEFAULT_TIMEOUT", default = "30")]
    pub crm_default_timeout_seconds: u64,

    #[envconfig(from = "CRM_DEFAULT_RETRY_ATTEMPTS", default = "3")]
    pub crm_default_retry_attempts: u32,

    #[envconfig(from = "CRM_HEALTH_CHECK_INTERVAL", default = "60")]
    pub crm_health_check_interval_seconds: u64,

    #[envconfig(from = "CRM_CIRCUIT_BREAKER_FAILURE_THRESHOLD", default = "5")]
    pub crm_circuit_breaker_failure_threshold: u32,

    #[envconfig(from = "CRM_CIRCUIT_BREAKER_TIMEOUT", default = "60")]
    pub crm_circuit_breaker_timeout_seconds: u64,

    // CRM manager enhancements
    #[envconfig(from = "CRM_HEALTH_CACHE_TTL", default = "15")]
    pub crm_health_cache_ttl_seconds: u64,

    // Deployment policy
    #[envconfig(from = "DEPLOY_MAX_RETRIES", default = "2")]
    pub deploy_max_retries: u32,

    #[envconfig(from = "DEPLOY_ROLLBACK_ON_PARTIAL", default = "false")]
    pub deploy_rollback_on_partial: bool,

    // Package delete behavior
    #[envconfig(from = "PACKAGE_DELETE_CASCADE", default = "false")]
    pub package_delete_cascade: bool,

    // Observability configuration
    #[envconfig(from = "OBSERVABILITY_ENABLED", default = "true")]
    pub observability_enabled: bool,

    #[envconfig(from = "LOG_LEVEL", default = "info")]
    pub log_level: String,

    #[envconfig(from = "LOG_FORMAT", default = "json")]
    pub log_format: String,

    #[envconfig(from = "METRICS_ENABLED", default = "true")]
    pub metrics_enabled: bool,

    #[envconfig(from = "METRICS_PORT", default = "9090")]
    pub metrics_port: u16,

    #[envconfig(from = "TRACING_ENABLED", default = "false")]
    pub tracing_enabled: bool,

    #[envconfig(from = "TRACING_ENDPOINT")]
    pub tracing_endpoint: Option<String>,
}

impl AppConfig {
    /// Load configuration from environment variables only
    pub fn load_from_env() -> Result<Self> {
        Ok(Self::init_from_env()?)
    }

    // Helper methods to get derived configurations
    pub fn server(&self) -> ServerConfig {
        ServerConfig {
            host: self.server_host.clone(),
            port: self.server_port,
            workers: self.server_workers,
        }
    }

    pub fn storage(&self) -> StorageConfig {
        let storage_type = match self.storage_type.to_lowercase().as_str() {
            "etcd" => StorageType::Etcd,
            other => {
                warn!(
                    "Unrecognized storage type '{}', falling back to 'memory'.",
                    other
                );
                StorageType::Memory
            }
        };

        StorageConfig {
            storage_type: storage_type.clone(),
            etcd: if matches!(storage_type, StorageType::Etcd) {
                Some(EtcdConfig {
                    endpoints: self
                        .etcd_endpoints
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect(),
                    key_prefix: self.etcd_key_prefix.clone(),
                    username: self.etcd_username.clone(),
                    password: self.etcd_password.clone(),
                    timeout: Some(self.etcd_timeout_seconds),
                    tls: if self.etcd_tls_enabled {
                        Some(EtcdTlsConfig {
                            ca_cert: self.etcd_ca_cert_path.clone(),
                            client_cert: self.etcd_client_cert_path.clone(),
                            client_key: self.etcd_client_key_path.clone(),
                            insecure: self.etcd_tls_insecure,
                        })
                    } else {
                        None
                    },
                })
            } else {
                None
            },
            memory: if matches!(storage_type, StorageType::Memory) {
                Some(MemoryConfig {})
            } else {
                None
            },
        }
    }

    pub fn crm(&self) -> CrmManagerConfig {
        let mut clusters = HashMap::new();

        // If default URL is provided, create a default cluster
        if let Some(url) = &self.crm_default_url {
            clusters.insert(
                "default".to_string(),
                CrmClientConfig {
                    url: url.clone(),
                    timeout: Some(self.crm_default_timeout_seconds),
                    retry_attempts: self.crm_default_retry_attempts,
                    api_key: None,
                    tls: None,
                },
            );
        }

        CrmManagerConfig {
            clusters,
            default_cluster: if self.crm_default_url.is_some() {
                Some("default".to_string())
            } else {
                None
            },
            health_check_interval: Some(self.crm_health_check_interval_seconds),
            circuit_breaker: Some(CircuitBreakerConfig {
                failure_threshold: self.crm_circuit_breaker_failure_threshold,
                timeout: self.crm_circuit_breaker_timeout_seconds,
            }),
            health_cache_ttl_seconds: self.crm_health_cache_ttl_seconds,
        }
    }

    pub fn deployment_policy(&self) -> DeploymentPolicyConfig {
        DeploymentPolicyConfig {
            max_retries: self.deploy_max_retries,
            rollback_on_partial: self.deploy_rollback_on_partial,
            package_delete_cascade: self.package_delete_cascade,
        }
    }

    pub fn observability(&self) -> ObservabilityConfig {
        ObservabilityConfig {
            enabled: self.observability_enabled,
            logging: LoggingConfig {
                level: self.log_level.clone(),
                format: self.log_format.clone(),
            },
            metrics: if self.metrics_enabled {
                Some(MetricsConfig {
                    port: self.metrics_port,
                })
            } else {
                None
            },
            tracing: if self.tracing_enabled {
                Some(TracingConfig {
                    endpoint: self.tracing_endpoint.clone(),
                })
            } else {
                None
            },
        }
    }
}

// Legacy structs for backwards compatibility
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub storage_type: StorageType,
    pub etcd: Option<EtcdConfig>,
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Clone)]
pub enum StorageType {
    Etcd,
    Memory,
}

#[derive(Debug, Clone)]
pub struct EtcdConfig {
    pub endpoints: Vec<String>,
    pub key_prefix: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub timeout: Option<u64>, // seconds
    pub tls: Option<EtcdTlsConfig>,
}

#[derive(Debug, Clone)]
pub struct EtcdTlsConfig {
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub insecure: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryConfig {
    // No specific config needed for memory storage
}

#[derive(Debug, Clone)]
pub struct CrmManagerConfig {
    pub clusters: HashMap<String, CrmClientConfig>,
    pub default_cluster: Option<String>,
    pub health_check_interval: Option<u64>, // seconds
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub health_cache_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct CrmClientConfig {
    pub url: String,
    pub timeout: Option<u64>, // seconds
    pub retry_attempts: u32,
    pub api_key: Option<String>,
    pub tls: Option<CrmTlsConfig>,
}

#[derive(Debug, Clone)]
pub struct CrmTlsConfig {
    pub ca_cert: Option<String>,
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
    pub insecure: bool,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub timeout: u64, // seconds
}

#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    pub enabled: bool,
    pub logging: LoggingConfig,
    pub metrics: Option<MetricsConfig>,
    pub tracing: Option<TracingConfig>,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone)]
pub struct MetricsConfig {
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeploymentPolicyConfig {
    pub max_retries: u32,
    pub rollback_on_partial: bool,
    pub package_delete_cascade: bool,
}
