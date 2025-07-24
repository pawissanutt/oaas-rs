use super::{file, CliConfig, ContextConfig};
use anyhow::Result;

/// Context management operations
pub struct ContextManager {
    config: CliConfig,
}

impl ContextManager {
    /// Create a new context manager with loaded configuration
    pub async fn new() -> Result<Self> {
        let config = super::load_or_create_config().await?;
        Ok(Self { config })
    }

    /// Get the current configuration
    pub fn config(&self) -> &CliConfig {
        &self.config
    }

    /// Get mutable reference to configuration
    pub fn config_mut(&mut self) -> &mut CliConfig {
        &mut self.config
    }

    /// Save configuration changes
    pub async fn save(&self) -> Result<()> {
        file::save_config(&self.config).await
    }

    /// Set context values
    pub async fn set_context(
        &mut self,
        name: Option<String>,
        pm_url: Option<String>,
        gateway_url: Option<String>,
        default_class: Option<String>,
    ) -> Result<()> {
        let context_name = name.unwrap_or_else(|| self.config.current_context.clone());
        
        // Get existing context or create new one
        let mut context = self.config
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

        // Save context
        self.config.set_context(context_name.clone(), context);
        
        // If this is a new context and no current context exists, make it current
        if !self.config.contexts.contains_key(&self.config.current_context) {
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
        }
    }
}
