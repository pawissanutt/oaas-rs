use crate::loader::ConfigError;
use crate::types::*;

pub trait Validate {
    fn validate(&self) -> Result<(), ConfigError>;
}

impl Validate for ServerConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.host.is_empty() {
            return Err(ConfigError::ValidationError(
                "Server host cannot be empty".to_string(),
            ));
        }

        if self.port == 0 {
            return Err(ConfigError::ValidationError(
                "Server port cannot be 0".to_string(),
            ));
        }

        if let Some(workers) = self.workers {
            if workers == 0 {
                return Err(ConfigError::ValidationError(
                    "Worker count cannot be 0".to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl Validate for TlsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.cert_file.is_empty() {
            return Err(ConfigError::ValidationError(
                "TLS cert file cannot be empty".to_string(),
            ));
        }

        if self.key_file.is_empty() {
            return Err(ConfigError::ValidationError(
                "TLS key file cannot be empty".to_string(),
            ));
        }

        // Check if files exist
        if !std::path::Path::new(&self.cert_file).exists() {
            return Err(ConfigError::ValidationError(format!(
                "TLS cert file not found: {}",
                self.cert_file
            )));
        }

        if !std::path::Path::new(&self.key_file).exists() {
            return Err(ConfigError::ValidationError(format!(
                "TLS key file not found: {}",
                self.key_file
            )));
        }

        Ok(())
    }
}

impl Validate for EtcdConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.endpoints.is_empty() {
            return Err(ConfigError::ValidationError(
                "etcd endpoints cannot be empty".to_string(),
            ));
        }

        for endpoint in &self.endpoints {
            if endpoint.is_empty() {
                return Err(ConfigError::ValidationError(
                    "etcd endpoint cannot be empty".to_string(),
                ));
            }
        }

        if self.key_prefix.is_empty() {
            return Err(ConfigError::ValidationError(
                "etcd key prefix cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}

impl Validate for ServiceConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationError(
                "Service name cannot be empty".to_string(),
            ));
        }

        self.server.validate()?;

        if let Some(ref etcd_config) = self.storage.etcd {
            etcd_config.validate()?;
        }

        Ok(())
    }
}
