use anyhow::{Context, Result};
use duct::cmd;
use tracing::{debug, info, instrument};

use crate::config::TestConfig;

pub struct OaasCli {
    binary_path: String,
    config_path: std::path::PathBuf,
}

impl OaasCli {
    pub fn new(config: TestConfig) -> Self {
        // Assuming we are running from workspace root
        let path = format!("target/{}/oprc-cli", config.build_profile);
        let config_path = std::env::current_dir()
            .unwrap()
            .join("target/system-e2e-config.yaml");
        // Ensure we start fresh
        if config_path.exists() {
            let _ = std::fs::remove_file(&config_path);
        }
        Self {
            binary_path: path,
            config_path,
        }
    }

    #[instrument(skip(self))]
    pub fn apply_package(&self, package_path: &str) -> Result<()> {
        info!("Applying package from '{}'...", package_path);
        let abs_path = std::fs::canonicalize(package_path).context(format!(
            "Failed to find package file: {}",
            package_path
        ))?;

        cmd!(
            &self.binary_path,
            "package",
            "apply",
            abs_path,
            "--overwrite"
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .run()
        .context("Failed to apply package")?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn apply_deployment(&self, deployment_path: &str) -> Result<()> {
        info!("Applying deployment from '{}'...", deployment_path);
        let abs_path = std::fs::canonicalize(deployment_path).context(
            format!("Failed to find deployment file: {}", deployment_path),
        )?;

        cmd!(
            &self.binary_path,
            "deploy",
            "apply",
            abs_path,
            "--overwrite"
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .run()
        .context("Failed to apply deployment")?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn invoke(
        &self,
        cls: &str,
        partition: &str,
        func: &str,
        payload: &str,
    ) -> Result<String> {
        info!("Invoking function {}/{}/{}...", cls, partition, func);
        let output = cmd!(
            &self.binary_path,
            "invoke",
            "-c",
            cls,
            partition,
            func,
            "-p",
            "-"
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .stdin_bytes(payload)
        .read()
        .context("Failed to invoke function")?;

        debug!("Invocation output: {}", output);
        Ok(output)
    }

    #[instrument(skip(self))]
    pub fn set_context(&self, pm_url: &str, gateway_url: &str) -> Result<()> {
        info!("Setting context: PM={}, Gateway={}", pm_url, gateway_url);
        cmd!(
            &self.binary_path,
            "context",
            "set",
            "--pm",
            pm_url,
            "--gateway",
            gateway_url,
            "--use-grpc",
            "true"
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .run()
        .context("Failed to set context")?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn object_set(
        &self,
        cls: &str,
        partition: &str,
        obj_id: &str,
        key: &str,
        value: &str,
    ) -> Result<()> {
        info!(
            "Setting object {}/{}/{} key={}...",
            cls, partition, obj_id, key
        );
        cmd!(
            &self.binary_path,
            "object",
            "set",
            partition,
            obj_id,
            "--cls-id",
            cls,
            "-s",
            format!("{}={}", key, value)
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .run()
        .context("Failed to set object")?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn object_get(
        &self,
        cls: &str,
        partition: &str,
        obj_id: &str,
        key: &str,
    ) -> Result<String> {
        info!(
            "Getting object {}/{}/{} key={}...",
            cls, partition, obj_id, key
        );
        let output = cmd!(
            &self.binary_path,
            "object",
            "get",
            partition,
            obj_id,
            "--cls-id",
            cls,
            "--key",
            key
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .read()
        .context("Failed to get object")?;
        debug!("Get object output: {}", output);
        Ok(output)
    }

    #[instrument(skip(self))]
    pub fn object_get_all(
        &self,
        cls: &str,
        partition: &str,
        obj_id: &str,
    ) -> Result<String> {
        info!("Getting object {}/{}/{}...", cls, partition, obj_id);
        let output = cmd!(
            &self.binary_path,
            "object",
            "get",
            partition,
            obj_id,
            "--cls-id",
            cls
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .read()
        .context("Failed to get object")?;
        debug!("Get object output: {}", output);
        Ok(output)
    }

    #[instrument(skip(self))]
    pub fn invoke_obj(
        &self,
        cls: &str,
        partition: &str,
        obj_id: &str,
        func: &str,
        payload: &str,
    ) -> Result<String> {
        info!(
            "Invoking object function {}/{}/{}/{}...",
            cls, partition, obj_id, func
        );
        let output = cmd!(
            &self.binary_path,
            "invoke",
            "-c",
            cls,
            partition,
            func,
            "-o",
            obj_id,
            "-p",
            "-"
        )
        .env("OPRC_CONFIG_PATH", &self.config_path)
        .env_remove("OPRC_ZENOH_PEERS")
        .stdin_bytes(payload)
        .read()
        .context("Failed to invoke object function")?;

        debug!("Invocation output: {}", output);
        Ok(output)
    }

    #[instrument(skip(self))]
    pub fn delete_package(&self, package_path: &str) -> Result<()> {
        info!("Deleting package from '{}'...", package_path);
        let abs_path = std::fs::canonicalize(package_path).context(format!(
            "Failed to find package file: {}",
            package_path
        ))?;

        cmd!(&self.binary_path, "package", "delete", abs_path, "-d")
            .env("OPRC_CONFIG_PATH", &self.config_path)
            .env_remove("OPRC_ZENOH_PEERS")
            .run()
            .context("Failed to delete package")?;
        Ok(())
    }
}
