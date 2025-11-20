// Infrastructure setup for E2E tests
// Handles Kind cluster creation, image loading, and OaaS deployment

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command as StdCommand;

/// Check if required CLI tools are available
pub fn check_prerequisites() -> Result<()> {
    let tools = vec!["kind", "kubectl", "docker"];
    
    for tool in tools {
        if !is_tool_available(tool) {
            bail!("{} is not available in PATH", tool);
        }
    }
    
    tracing::info!("All prerequisites are available");
    Ok(())
}

fn is_tool_available(tool: &str) -> bool {
    StdCommand::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Create a Kind cluster with the specified name
pub fn create_kind_cluster(cluster_name: &str) -> Result<()> {
    tracing::info!("Creating Kind cluster: {}", cluster_name);
    
    // Check if cluster already exists
    if cluster_exists(cluster_name)? {
        tracing::info!("Cluster {} already exists, deleting first", cluster_name);
        delete_kind_cluster(cluster_name)?;
    }
    
    let output = duct::cmd!("kind", "create", "cluster", "--name", cluster_name)
        .stdout_capture()
        .stderr_capture()
        .run()
        .context("Failed to create Kind cluster")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("kind create cluster failed: {}", stderr);
    }
    
    tracing::info!("Kind cluster {} created successfully", cluster_name);
    Ok(())
}

/// Check if a Kind cluster exists
pub fn cluster_exists(cluster_name: &str) -> Result<bool> {
    let output = duct::cmd!("kind", "get", "clusters")
        .stdout_capture()
        .stderr_capture()
        .run()?;
    
    let clusters = String::from_utf8_lossy(&output.stdout);
    Ok(clusters.lines().any(|line| line.trim() == cluster_name))
}

/// Delete a Kind cluster
pub fn delete_kind_cluster(cluster_name: &str) -> Result<()> {
    tracing::info!("Deleting Kind cluster: {}", cluster_name);
    
    let output = duct::cmd!("kind", "delete", "cluster", "--name", cluster_name)
        .stdout_capture()
        .stderr_capture()
        .run()
        .context("Failed to delete Kind cluster")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("kind delete cluster warning: {}", stderr);
    }
    
    tracing::info!("Kind cluster {} deleted", cluster_name);
    Ok(())
}

/// Build Docker images using docker compose
pub fn build_images(repo_root: &Path) -> Result<()> {
    tracing::info!("Building Docker images...");
    
    let output = duct::cmd!(
        "docker", "compose", 
        "-f", "docker-compose.build.yml",
        "build"
    )
    .dir(repo_root)
    .stdout_capture()
    .stderr_capture()
    .run()
    .context("Failed to build images")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("docker compose build failed: {}", stderr);
    }
    
    tracing::info!("Docker images built successfully");
    Ok(())
}

/// Load a Docker image into Kind cluster
pub fn load_image_to_kind(cluster_name: &str, image: &str) -> Result<()> {
    tracing::info!("Loading image {} into Kind cluster {}", image, cluster_name);
    
    let output = duct::cmd!("kind", "load", "docker-image", image, "--name", cluster_name)
        .stdout_capture()
        .stderr_capture()
        .run()
        .context(format!("Failed to load image {} to Kind", image))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("kind load docker-image failed for {}: {}", image, stderr);
    }
    
    Ok(())
}

/// Load all required images into Kind cluster
pub fn load_all_images(cluster_name: &str, images: &[&str]) -> Result<()> {
    for image in images {
        load_image_to_kind(cluster_name, image)?;
    }
    tracing::info!("All images loaded successfully");
    Ok(())
}

/// Deploy OaaS components using the deploy script
pub fn deploy_oaas(repo_root: &Path) -> Result<()> {
    tracing::info!("Deploying OaaS components...");
    
    let deploy_script = repo_root.join("k8s/charts/deploy.sh");
    if !deploy_script.exists() {
        bail!("Deploy script not found at {:?}", deploy_script);
    }
    
    // Run the deploy script with default settings
    let output = duct::cmd!("bash", deploy_script, "deploy")
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .run()
        .context("Failed to deploy OaaS")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("deploy.sh failed: {}", stderr);
    }
    
    tracing::info!("OaaS components deployed successfully");
    Ok(())
}

/// Undeploy OaaS components
pub fn undeploy_oaas(repo_root: &Path) -> Result<()> {
    tracing::info!("Undeploying OaaS components...");
    
    let deploy_script = repo_root.join("k8s/charts/deploy.sh");
    if !deploy_script.exists() {
        tracing::warn!("Deploy script not found, skipping undeploy");
        return Ok(());
    }
    
    let output = duct::cmd!("bash", deploy_script, "undeploy")
        .dir(repo_root)
        .stdout_capture()
        .stderr_capture()
        .run()
        .context("Failed to undeploy OaaS")?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("undeploy.sh warning: {}", stderr);
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tool_available() {
        // Test with a tool that should exist
        assert!(is_tool_available("sh"));
    }
}
