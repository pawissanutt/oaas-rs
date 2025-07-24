use super::CliConfig;
use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::fs;

/// Get the configuration directory path
pub fn get_config_dir() -> Result<PathBuf> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home_dir.join(".oprc"))
}

/// Get the configuration file path
pub fn get_config_file_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("config.yml"))
}

/// Load configuration from file
pub async fn load_config() -> Result<CliConfig> {
    let config_path = get_config_file_path()?;
    
    if !config_path.exists() {
        return Err(anyhow::anyhow!("Configuration file does not exist"));
    }

    let content = fs::read_to_string(&config_path)
        .await
        .with_context(|| format!("Failed to read config file: {:?}", config_path))?;

    let config: CliConfig = serde_yaml::from_str(&content)
        .with_context(|| "Failed to parse configuration file")?;

    Ok(config)
}

/// Save configuration to file
pub async fn save_config(config: &CliConfig) -> Result<()> {
    let config_dir = get_config_dir()?;
    let config_path = get_config_file_path()?;

    // Create config directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .await
            .with_context(|| format!("Failed to create config directory: {:?}", config_dir))?;
    }

    let content = serde_yaml::to_string(config)
        .with_context(|| "Failed to serialize configuration")?;

    fs::write(&config_path, content)
        .await
        .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

    Ok(())
}
