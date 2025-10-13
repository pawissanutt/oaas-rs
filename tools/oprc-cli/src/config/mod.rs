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
    pub zenoh_peer: Option<String>,
    /// Explicit transport selector: when Some(true) use gRPC, when Some(false) use Zenoh; None => infer
    pub use_grpc: Option<bool>,
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
                zenoh_peer: None,
                use_grpc: None,
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

    #[allow(unused)]
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

/// Load or create configuration from a specific path
pub async fn load_or_create_config_from_path(
    config_path: &std::path::Path,
) -> Result<CliConfig> {
    match file::load_config_from_path(config_path).await {
        Ok(config) => Ok(config),
        Err(_) => {
            let config = CliConfig::default();
            file::save_config_to_path(&config, config_path).await?;
            Ok(config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio;

    // Helper function to create a temporary config directory
    async fn create_test_config()
    -> (TempDir, std::path::PathBuf, ContextManager) {
        let temp_dir = TempDir::new().unwrap();

        // Set up test config
        let mut contexts = HashMap::new();
        contexts.insert(
            "test".to_string(),
            ContextConfig {
                pm_url: Some("http://test.pm.com".to_string()),
                gateway_url: Some("http://test.gateway.com".to_string()),
                default_class: Some("test.class".to_string()),
                zenoh_peer: Some("tcp/192.168.1.100:7447".to_string()),
                use_grpc: Some(false),
            },
        );
        contexts.insert(
            "prod".to_string(),
            ContextConfig {
                pm_url: Some("http://prod.pm.com".to_string()),
                gateway_url: Some("http://prod.gateway.com".to_string()),
                default_class: Some("prod.class".to_string()),
                zenoh_peer: None,
                use_grpc: Some(true),
            },
        );

        let config = CliConfig {
            contexts,
            current_context: "test".to_string(),
        };

        // Save config to temp directory
        let config_path = temp_dir.path().join("config.yml");
        let config_content = serde_yaml::to_string(&config).unwrap();
        tokio::fs::write(&config_path, config_content)
            .await
            .unwrap();

        let manager = ContextManager::with_config_path(&config_path)
            .await
            .unwrap();
        (temp_dir, config_path, manager)
    }

    #[tokio::test]
    async fn test_context_config_structure() {
        let context = ContextConfig {
            pm_url: Some("http://test.com".to_string()),
            gateway_url: Some("http://gateway.com".to_string()),
            default_class: Some("test.class".to_string()),
            zenoh_peer: Some("tcp/localhost:7447".to_string()),
            use_grpc: None,
        };

        assert_eq!(context.pm_url.unwrap(), "http://test.com");
        assert_eq!(context.gateway_url.unwrap(), "http://gateway.com");
        assert_eq!(context.default_class.unwrap(), "test.class");
        assert_eq!(context.zenoh_peer.unwrap(), "tcp/localhost:7447");
    }

    #[tokio::test]
    async fn test_context_manager_creation() {
        let (_temp_dir, _config_path, manager) = create_test_config().await;

        assert_eq!(manager.config().current_context, "test");
        assert!(manager.config().contexts.contains_key("test"));
        assert!(manager.config().contexts.contains_key("prod"));
    }

    #[tokio::test]
    async fn test_context_switching() {
        let (_temp_dir, _config_path, mut manager) = create_test_config().await;

        // Switch to prod context
        manager.select_context("prod".to_string()).await.unwrap();
        assert_eq!(manager.config().current_context, "prod");

        // Get current context
        let current = manager.get_current_context().unwrap();
        assert_eq!(current.pm_url.as_ref().unwrap(), "http://prod.pm.com");
        assert_eq!(current.zenoh_peer, None);
    }

    #[tokio::test]
    async fn test_context_setting() {
        let (temp_dir, _config_path, mut manager) = create_test_config().await;

        // Set new context values
        let result = manager
            .set_context(
                Some("new_test".to_string()),
                Some("http://new.pm.com".to_string()),
                Some("http://new.gateway.com".to_string()),
                Some("new.class".to_string()),
                Some("tcp/new.host:7447".to_string()),
                Some(false),
            )
            .await;

        if let Err(e) = &result {
            println!("Set context error: {:?}", e);
        }
        result.unwrap();

        // Verify the new context was created
        let new_context = manager.config().get_context("new_test").unwrap();
        assert_eq!(new_context.pm_url.as_ref().unwrap(), "http://new.pm.com");
        assert_eq!(
            new_context.gateway_url.as_ref().unwrap(),
            "http://new.gateway.com"
        );
        assert_eq!(new_context.default_class.as_ref().unwrap(), "new.class");
        assert_eq!(
            new_context.zenoh_peer.as_ref().unwrap(),
            "tcp/new.host:7447"
        );

        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_context_serialization() {
        let mut contexts = HashMap::new();
        contexts.insert(
            "test".to_string(),
            ContextConfig {
                pm_url: Some("http://test.pm.com".to_string()),
                gateway_url: Some("http://test.gateway.com".to_string()),
                default_class: Some("test.class".to_string()),
                zenoh_peer: Some("tcp/192.168.1.100:7447".to_string()),
                use_grpc: None,
            },
        );

        let config = CliConfig {
            contexts,
            current_context: "test".to_string(),
        };

        // Test YAML serialization
        let yaml_str = serde_yaml::to_string(&config).unwrap();
        let deserialized: CliConfig = serde_yaml::from_str(&yaml_str).unwrap();

        assert_eq!(config.current_context, deserialized.current_context);
        assert_eq!(
            config.contexts.get("test").unwrap().pm_url,
            deserialized.contexts.get("test").unwrap().pm_url
        );
        assert_eq!(
            config.contexts.get("test").unwrap().zenoh_peer,
            deserialized.contexts.get("test").unwrap().zenoh_peer
        );

        // Test JSON serialization
        let json_str = serde_json::to_string(&config).unwrap();
        let deserialized_json: CliConfig =
            serde_json::from_str(&json_str).unwrap();

        assert_eq!(config.current_context, deserialized_json.current_context);
    }
}
