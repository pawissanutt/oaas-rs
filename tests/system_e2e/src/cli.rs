// CLI wrapper for oprc-cli commands

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

/// Wrapper for oprc-cli commands
pub struct CliWrapper {
    config_dir: Option<TempDir>,
    cli_path: String,
}

impl CliWrapper {
    /// Create a new CLI wrapper
    /// If config_path is None, a temporary config will be created
    pub fn new() -> Result<Self> {
        // Find oprc-cli binary (assume it's in PATH or built)
        let cli_path = which::which("oprc-cli")
            .or_else(|_| {
                // Try to find in target directory
                let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
                    .unwrap_or_else(|_| ".".to_string());
                let root = Path::new(&manifest_dir).parent().unwrap().parent().unwrap();
                let debug_path = root.join("target/debug/oprc-cli");
                let release_path = root.join("target/release/oprc-cli");
                
                if release_path.exists() {
                    Ok(release_path)
                } else if debug_path.exists() {
                    Ok(debug_path)
                } else {
                    Err(which::Error::CannotFindBinaryPath)
                }
            })
            .context("oprc-cli not found in PATH or target directory")?;
        
        Ok(Self {
            config_dir: None,
            cli_path: cli_path.to_string_lossy().to_string(),
        })
    }

    /// Set PM URL in the CLI configuration
    pub fn configure_pm(&mut self, pm_url: &str) -> Result<()> {
        let config_dir = TempDir::new()?;
        let config_path = config_dir.path().join("config.yaml");
        
        let config = serde_yaml::to_string(&serde_json::json!({
            "contexts": {
                "default": {
                    "pm_url": pm_url,
                }
            },
            "current_context": "default"
        }))?;
        
        std::fs::write(&config_path, config)?;
        self.config_dir = Some(config_dir);
        
        Ok(())
    }

    /// Apply a package from a YAML file
    pub fn pkg_apply(&self, package_file: &Path) -> Result<String> {
        tracing::info!("Applying package from {:?}", package_file);
        
        let mut cmd = duct::cmd!(&self.cli_path, "pkg", "apply", package_file);
        
        if let Some(ref config_dir) = self.config_dir {
            cmd = cmd.env("OPRC_CONFIG_HOME", config_dir.path());
        }
        
        let output = cmd
            .stdout_capture()
            .stderr_capture()
            .run()
            .context("Failed to run oprc-cli pkg apply")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("oprc-cli pkg apply failed: {}", stderr);
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Apply a deployment from a YAML file
    pub fn dep_apply(&self, deployment_file: &Path) -> Result<String> {
        tracing::info!("Applying deployment from {:?}", deployment_file);
        
        let mut cmd = duct::cmd!(&self.cli_path, "dep", "apply", deployment_file);
        
        if let Some(ref config_dir) = self.config_dir {
            cmd = cmd.env("OPRC_CONFIG_HOME", config_dir.path());
        }
        
        let output = cmd
            .stdout_capture()
            .stderr_capture()
            .run()
            .context("Failed to run oprc-cli dep apply")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("oprc-cli dep apply failed: {}", stderr);
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Invoke a function
    pub fn invoke(
        &self,
        class: &str,
        id: &str,
        func: &str,
        data: Option<&str>,
    ) -> Result<String> {
        tracing::info!("Invoking function {}.{}.{}", class, id, func);
        
        let mut args = vec!["invoke", class, id, func];
        
        let data_arg;
        if let Some(d) = data {
            args.push("-d");
            data_arg = d.to_string();
            args.push(&data_arg);
        }
        
        let mut cmd = duct::cmd(&self.cli_path, &args);
        
        if let Some(ref config_dir) = self.config_dir {
            cmd = cmd.env("OPRC_CONFIG_HOME", config_dir.path());
        }
        
        let output = cmd
            .stdout_capture()
            .stderr_capture()
            .run()
            .context("Failed to run oprc-cli invoke")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("oprc-cli invoke failed: {}", stderr);
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get package information
    pub fn pkg_get(&self, package_name: &str) -> Result<Value> {
        tracing::info!("Getting package info for {}", package_name);
        
        let mut cmd = duct::cmd!(&self.cli_path, "pkg", "get", package_name, "-o", "json");
        
        if let Some(ref config_dir) = self.config_dir {
            cmd = cmd.env("OPRC_CONFIG_HOME", config_dir.path());
        }
        
        let output = cmd
            .stdout_capture()
            .stderr_capture()
            .run()
            .context("Failed to run oprc-cli pkg get")?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("oprc-cli pkg get failed: {}", stderr);
        }
        
        let json_str = String::from_utf8_lossy(&output.stdout);
        let value: Value = serde_json::from_str(&json_str)
            .context("Failed to parse JSON response")?;
        
        Ok(value)
    }
}

impl Default for CliWrapper {
    fn default() -> Self {
        Self::new().expect("Failed to create CLI wrapper")
    }
}
