use serde::de::DeserializeOwned;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found: {0}")]
    FileNotFound(String),

    #[error("Configuration parsing error: {0}")]
    ParseError(#[from] config::ConfigError),

    #[error("Environment variable error: {0}")]
    EnvError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),
}

pub struct ConfigLoader {
    config_dir: String,
    environment: String,
}

impl ConfigLoader {
    pub fn new(config_dir: String, environment: String) -> Self {
        Self {
            config_dir,
            environment,
        }
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        let config_dir = std::env::var("OAAS_CONFIG_DIR")
            .unwrap_or_else(|_| "config".to_string());
        let environment = std::env::var("OAAS_ENV")
            .unwrap_or_else(|_| "development".to_string());

        Ok(Self::new(config_dir, environment))
    }

    pub fn load<T: DeserializeOwned>(&self) -> Result<T, ConfigError> {
        let default_path = format!("{}/default.yaml", self.config_dir);
        let env_path = format!("{}/{}.yaml", self.config_dir, self.environment);

        let mut config = config::Config::builder();

        // Load default configuration
        if Path::new(&default_path).exists() {
            config = config.add_source(config::File::with_name(&default_path));
        }

        // Override with environment-specific configuration
        if Path::new(&env_path).exists() {
            config = config.add_source(config::File::with_name(&env_path));
        }

        // Override with environment variables
        config = config.add_source(
            config::Environment::with_prefix("OAAS")
                .prefix_separator("_")
                .separator("__"),
        );

        let config = config.build()?;
        let parsed_config: T = config.try_deserialize()?;

        Ok(parsed_config)
    }
}

pub fn load_config<T: DeserializeOwned>() -> Result<T, ConfigError> {
    let loader = ConfigLoader::from_env()?;
    loader.load()
}
