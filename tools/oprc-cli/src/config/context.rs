use super::{CliConfig, ContextConfig, file};
use anyhow::Result;
use std::path::Path;

/// Context management operations
pub struct ContextManager {
    config: CliConfig,
    config_path: Option<std::path::PathBuf>,
}

impl ContextManager {
    /// Create a new context manager with loaded configuration
    pub async fn new() -> Result<Self> {
        let config = super::load_or_create_config().await?;
        Ok(Self {
            config,
            config_path: None,
        })
    }

    #[allow(unused)]
    /// Create a new context manager with a specific config path (useful for testing)
    pub async fn with_config_path<P: AsRef<Path>>(
        config_path: P,
    ) -> Result<Self> {
        let config =
            super::load_or_create_config_from_path(config_path.as_ref())
                .await?;
        Ok(Self {
            config,
            config_path: Some(config_path.as_ref().to_path_buf()),
        })
    }

    /// Get the current configuration
    pub fn config(&self) -> &CliConfig {
        &self.config
    }

    #[allow(dead_code)]
    /// Get mutable reference to configuration
    pub fn config_mut(&mut self) -> &mut CliConfig {
        &mut self.config
    }

    /// Save configuration changes
    pub async fn save(&self) -> Result<()> {
        if let Some(config_path) = &self.config_path {
            file::save_config_to_path(&self.config, config_path).await
        } else {
            file::save_config(&self.config).await
        }
    }

    /// Set context values
    pub async fn set_context(
        &mut self,
        name: Option<String>,
        pm_url: Option<String>,
        gateway_url: Option<String>,
        default_class: Option<String>,
        zenoh_peer: Option<String>,
    ) -> Result<()> {
        let context_name =
            name.unwrap_or_else(|| self.config.current_context.clone());

        // Get existing context or create new one
        let mut context = self
            .config
            .get_context(&context_name)
            .cloned()
            .unwrap_or_default();

        // Update provided fields
        if let Some(url) = pm_url {
            context.pm_url = Some(url);
        }
        if let Some(url) = gateway_url {
            context.gateway_url = Some(url);
        }
        if let Some(class) = default_class {
            context.default_class = Some(class);
        }
        if let Some(peer) = zenoh_peer {
            context.zenoh_peer = Some(peer);
        }

        // Save context
        self.config.set_context(context_name.clone(), context);

        // If this is a new context and no current context exists, make it current
        if !self
            .config
            .contexts
            .contains_key(&self.config.current_context)
        {
            self.config.current_context = context_name;
        }

        self.save().await
    }

    /// Switch to a different context
    pub async fn select_context(&mut self, name: String) -> Result<()> {
        self.config.set_current_context(name)?;
        self.save().await
    }

    /// Get current context configuration
    pub fn get_current_context(&self) -> Option<&ContextConfig> {
        self.config.current_context()
    }

    #[allow(unused)]
    /// List all available contexts
    pub fn list_contexts(&self) -> Vec<&String> {
        self.config.list_contexts()
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            pm_url: None,
            gateway_url: None,
            default_class: None,
            zenoh_peer: None,
        }
    }
}
