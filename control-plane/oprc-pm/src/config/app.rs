use anyhow::Result;
use envconfig::Envconfig;
use std::collections::HashMap;
use std::env;
use tracing::warn;

#[derive(Debug, Clone, Envconfig)]
pub struct AppConfig {
    // Server configuration
    #[envconfig(from = "SERVER_HOST", default = "0.0.0.0")]
    pub server_host: String,

    #[envconfig(from = "SERVER_PORT", default = "8080")]
    pub server_port: u16,

    #[envconfig(from = "SERVER_WORKERS")]
    pub server_workers: Option<usize>,

    #[envconfig(from = "STATIC_DIR", default = "static")]
    pub static_dir: String,

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

    // Multi-CRM configuration
    // Highest precedence: JSON definition
    #[envconfig(from = "CRM_CLUSTERS_JSON")]
    pub crm_clusters_json: Option<String>,
    // Env-list + per-cluster prefixed vars
    #[envconfig(from = "CRM_CLUSTERS")]
    pub crm_clusters: Option<String>,
    // Explicit default cluster (must exist among configured clusters)
    #[envconfig(from = "CRM_DEFAULT_CLUSTER")]
    pub crm_default_cluster: Option<String>,

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

    // Gateway configuration (for object proxy)
    #[envconfig(from = "GATEWAY_URL")]
    pub gateway_url: Option<String>,

    #[envconfig(from = "GATEWAY_TIMEOUT", default = "30")]
    pub gateway_timeout_seconds: u64,

    /// Maximum payload size for gateway proxy requests (default: 50MB)
    #[envconfig(from = "GATEWAY_MAX_PAYLOAD_BYTES", default = "52428800")]
    pub gateway_max_payload_bytes: usize,
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
            static_dir: self.static_dir.clone(),
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
        // Build clusters from highest precedence to lowest:
        // 1) JSON, 2) Env list + prefixes, 3) Legacy single default URL
        let mut clusters: HashMap<String, CrmClientConfig> = HashMap::new();

        if let Some(json) = &self.crm_clusters_json {
            match parse_clusters_from_json(json) {
                Ok(map) => clusters = map,
                Err(e) => {
                    panic!("Invalid CRM_CLUSTERS_JSON: {e}");
                }
            }
        } else if let Some(list) = &self.crm_clusters {
            match parse_clusters_from_env_list(list) {
                Ok(map) => clusters = map,
                Err(e) => {
                    panic!("Invalid CRM_CLUSTERS / prefixed envs: {e}");
                }
            }
        } else if let Some(url) = &self.crm_default_url {
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

        // Determine default cluster
        let default_cluster = self
            .crm_default_cluster
            .as_ref()
            .and_then(|name| {
                if clusters.contains_key(name) {
                    Some(name.clone())
                } else {
                    warn!("CRM_DEFAULT_CLUSTER='{}' not found among configured clusters; falling back.", name);
                    None
                }
            })
            .or_else(|| {
                // Prefer a cluster literally named "default" if present
                if clusters.contains_key("default") {
                    Some("default".to_string())
                } else if let Some((k, _)) = clusters.iter().next() {
                    // Any cluster (iteration order unspecified)
                    Some(k.clone())
                } else {
                    None
                }
            });

        CrmManagerConfig {
            clusters,
            default_cluster,
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

    pub fn gateway(&self) -> Option<GatewayProxyConfig> {
        self.gateway_url.as_ref().map(|url| GatewayProxyConfig {
            url: url.clone(),
            timeout_seconds: self.gateway_timeout_seconds,
            max_payload_bytes: self.gateway_max_payload_bytes,
        })
    }
}

/// Configuration for proxying requests to the Gateway service.
#[derive(Debug, Clone)]
pub struct GatewayProxyConfig {
    pub url: String,
    pub timeout_seconds: u64,
    pub max_payload_bytes: usize,
}

// Legacy structs for backwards compatibility
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: Option<usize>,
    pub static_dir: String,
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

#[derive(Debug, Clone, serde::Deserialize)]
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

// ---------- Helpers for multi-CRM parsing ----------
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct ClusterJsonConfig {
    url: String,
    timeout: Option<u64>,
    retry_attempts: Option<u32>,
    api_key: Option<String>,
    tls: Option<CrmTlsConfig>,
}

fn parse_clusters_from_json(
    json: &str,
) -> anyhow::Result<HashMap<String, CrmClientConfig>> {
    // Support either an array of {name, ...}? or { name: { ... } } and array of objects with explicit name field
    // We documented array of objects without name and object map; implement both:
    let v: serde_json::Value = serde_json::from_str(json)?;
    let mut out = HashMap::new();
    match v {
        serde_json::Value::Array(items) => {
            // Expect each item to have at least url and a name field optional; if name missing, error
            for (idx, item) in items.into_iter().enumerate() {
                // try to deserialize to a map first to extract name/url
                if let Some(obj) = item.as_object() {
                    let name = obj
                        .get("name")
                        .and_then(|n| n.as_str())
                        .ok_or_else(|| anyhow::anyhow!(
                            "CRM_CLUSTERS_JSON[{}] missing 'name' field in array form",
                            idx
                        ))?
                        .to_string();
                    let cfg: ClusterJsonConfig = serde_json::from_value(
                        serde_json::Value::Object(obj.clone()),
                    )?;
                    insert_cluster(&mut out, name, cfg)?;
                } else {
                    return Err(anyhow::anyhow!(
                        "Invalid JSON array element at index {}",
                        idx
                    ));
                }
            }
        }
        serde_json::Value::Object(map) => {
            for (name, val) in map.into_iter() {
                let cfg: ClusterJsonConfig = serde_json::from_value(val)?;
                insert_cluster(&mut out, name, cfg)?;
            }
        }
        _ => {
            return Err(anyhow::anyhow!(
                "CRM_CLUSTERS_JSON must be an array or object"
            ));
        }
    }
    Ok(out)
}

fn insert_cluster(
    out: &mut HashMap<String, CrmClientConfig>,
    name: String,
    cfg: ClusterJsonConfig,
) -> anyhow::Result<()> {
    validate_cluster_name(&name)?;
    if out.contains_key(&name) {
        return Err(anyhow::anyhow!("Duplicate cluster name '{}'", name));
    }
    out.insert(
        name,
        CrmClientConfig {
            url: cfg.url,
            timeout: cfg.timeout,
            retry_attempts: cfg.retry_attempts.unwrap_or(3),
            api_key: cfg.api_key,
            tls: cfg.tls,
        },
    );
    Ok(())
}

fn parse_clusters_from_env_list(
    list: &str,
) -> anyhow::Result<HashMap<String, CrmClientConfig>> {
    let mut out = HashMap::new();
    for raw in list.split(',') {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        validate_cluster_name(name)?;
        if out.contains_key(name) {
            return Err(anyhow::anyhow!(
                "Duplicate cluster name '{}' in CRM_CLUSTERS",
                name
            ));
        }
        let suffix = sanitize_env_suffix(name);
        let url_key = format!("CRM_CLUSTER_{}_URL", suffix);
        let url = env::var(&url_key).map_err(|_| {
            anyhow::anyhow!("Missing {} for cluster '{}'", url_key, name)
        })?;

        let timeout = read_env_u64(&format!("CRM_CLUSTER_{}_TIMEOUT", suffix));
        let retry_attempts =
            read_env_u32(&format!("CRM_CLUSTER_{}_RETRY_ATTEMPTS", suffix))
                .unwrap_or(3);
        let api_key = env::var(format!("CRM_CLUSTER_{}_API_KEY", suffix)).ok();

        let tls_enabled =
            read_env_bool(&format!("CRM_CLUSTER_{}_TLS_ENABLED", suffix))
                .unwrap_or(false);
        let tls = if tls_enabled
            || env::var(format!("CRM_CLUSTER_{}_TLS_CA_CERT_PATH", suffix))
                .is_ok()
            || env::var(format!("CRM_CLUSTER_{}_TLS_CLIENT_CERT_PATH", suffix))
                .is_ok()
            || env::var(format!("CRM_CLUSTER_{}_TLS_CLIENT_KEY_PATH", suffix))
                .is_ok()
            || read_env_bool(&format!("CRM_CLUSTER_{}_TLS_INSECURE", suffix))
                .is_some()
        {
            Some(CrmTlsConfig {
                ca_cert: env::var(format!(
                    "CRM_CLUSTER_{}_TLS_CA_CERT_PATH",
                    suffix
                ))
                .ok(),
                client_cert: env::var(format!(
                    "CRM_CLUSTER_{}_TLS_CLIENT_CERT_PATH",
                    suffix
                ))
                .ok(),
                client_key: env::var(format!(
                    "CRM_CLUSTER_{}_TLS_CLIENT_KEY_PATH",
                    suffix
                ))
                .ok(),
                insecure: read_env_bool(&format!(
                    "CRM_CLUSTER_{}_TLS_INSECURE",
                    suffix
                ))
                .unwrap_or(false),
            })
        } else {
            None
        };

        out.insert(
            name.to_string(),
            CrmClientConfig {
                url,
                timeout,
                retry_attempts,
                api_key,
                tls,
            },
        );
    }

    if out.is_empty() {
        return Err(anyhow::anyhow!("CRM_CLUSTERS resolved to zero clusters"));
    }

    Ok(out)
}

fn sanitize_env_suffix(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' => (c as u8 - b'a' + b'A') as char,
            'A'..='Z' | '0'..='9' => c,
            _ => '_',
        })
        .collect()
}

fn validate_cluster_name(name: &str) -> anyhow::Result<()> {
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        Err(anyhow::anyhow!(
            "Invalid cluster name '{}': only [A-Za-z0-9_-] allowed",
            name
        ))
    } else {
        Ok(())
    }
}

fn read_env_u64(key: &str) -> Option<u64> {
    env::var(key).ok().and_then(|v| v.parse::<u64>().ok())
}

fn read_env_u32(key: &str) -> Option<u32> {
    env::var(key).ok().and_then(|v| v.parse::<u32>().ok())
}

fn read_env_bool(key: &str) -> Option<bool> {
    env::var(key)
        .ok()
        .and_then(|v| match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}
