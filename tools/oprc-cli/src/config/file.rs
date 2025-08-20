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
    // Check for test environment variable first
    if let Ok(test_path) = std::env::var("OPRC_CONFIG_PATH") {
        return Ok(PathBuf::from(test_path));
    }
    Ok(get_config_dir()?.join("config.yml"))
}

/// Load configuration from file
pub async fn load_config() -> Result<CliConfig> {
    let config_path = get_config_file_path()?;

    if !config_path.exists() {
        return Err(anyhow::anyhow!("Configuration file does not exist"));
    }

    let content =
        fs::read_to_string(&config_path).await.with_context(|| {
            format!("Failed to read config file: {:?}", config_path)
        })?;

    let config: CliConfig = serde_yaml::from_str(&content)
        .with_context(|| "Failed to parse configuration file")?;

    Ok(config)
}

/// Save configuration to file
pub async fn save_config(config: &CliConfig) -> Result<()> {
    let config_path = get_config_file_path()?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create config directory: {:?}", parent)
            })?;
        }
    }

    let content = serde_yaml::to_string(config)
        .with_context(|| "Failed to serialize configuration")?;

    fs::write(&config_path, content).await.with_context(|| {
        format!("Failed to write config file: {:?}", config_path)
    })?;

    Ok(())
}

/// Load configuration from a specific file path
pub async fn load_config_from_path(
    config_path: &std::path::Path,
) -> Result<CliConfig> {
    if !config_path.exists() {
        return Err(anyhow::anyhow!("Configuration file does not exist"));
    }

    let content = fs::read_to_string(config_path).await.with_context(|| {
        format!("Failed to read config file: {:?}", config_path)
    })?;

    let config: CliConfig = serde_yaml::from_str(&content)
        .with_context(|| "Failed to parse configuration file")?;

    Ok(config)
}

/// Save configuration to a specific file path
pub async fn save_config_to_path(
    config: &CliConfig,
    config_path: &std::path::Path,
) -> Result<()> {
    // Create parent directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).await.with_context(|| {
                format!("Failed to create config directory: {:?}", parent)
            })?;
        }
    }

    let content = serde_yaml::to_string(config)
        .with_context(|| "Failed to serialize configuration")?;

    fs::write(config_path, content).await.with_context(|| {
        format!("Failed to write config file: {:?}", config_path)
    })?;

    Ok(())
}
