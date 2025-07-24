mod context;
mod file;

pub use context::*;
pub use file::*;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main CLI configuration structure
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CliConfig {
    pub contexts: HashMap<String, ContextConfig>,
    pub current_context: String,
}

/// Configuration for a specific context
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ContextConfig {
    pub pm_url: Option<String>,
    pub gateway_url: Option<String>,
    pub default_class: Option<String>,
}

impl Default for CliConfig {
    fn default() -> Self {
        let mut contexts = HashMap::new();
        contexts.insert(
            "default".to_string(),
            ContextConfig {
                pm_url: Some("http://pm.oaas.127.0.0.1.nip.io".to_string()),
                gateway_url: Some("http://oaas.127.0.0.1.nip.io".to_string()),
                default_class: Some("example.record".to_string()),
            },
        );

        Self {
            contexts,
            current_context: "default".to_string(),
        }
    }
}

impl CliConfig {
    /// Get the current context configuration
    pub fn current_context(&self) -> Option<&ContextConfig> {
        self.contexts.get(&self.current_context)
    }

    /// Get a specific context configuration
    pub fn get_context(&self, name: &str) -> Option<&ContextConfig> {
        self.contexts.get(name)
    }

    /// Set the current context
    pub fn set_current_context(&mut self, name: String) -> Result<()> {
        if !self.contexts.contains_key(&name) {
            return Err(anyhow::anyhow!("Context '{}' does not exist", name));
        }
        self.current_context = name;
        Ok(())
    }

    /// Update or create a context
    pub fn set_context(&mut self, name: String, config: ContextConfig) {
        self.contexts.insert(name, config);
    }

    /// List all context names
    pub fn list_contexts(&self) -> Vec<&String> {
        self.contexts.keys().collect()
    }
}

/// Load or create default configuration
pub async fn load_or_create_config() -> Result<CliConfig> {
    match load_config().await {
        Ok(config) => Ok(config),
        Err(_) => {
            let config = CliConfig::default();
            save_config(&config).await?;
            Ok(config)
        }
    }
}
