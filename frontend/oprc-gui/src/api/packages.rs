//! Packages listing and management

use crate::types::PackagesSnapshot;

pub async fn proxy_packages() -> Result<PackagesSnapshot, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/packages", base);

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "Packages request failed: {}",
            resp.status()
        ));
    }

    let pkgs: Vec<oprc_models::OPackage> =
        resp.json().await.map_err(|e| anyhow::anyhow!(e))?;

    Ok(crate::types::PackagesSnapshot {
        packages: pkgs,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

/// Response from package apply operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PackageApplyResponse {
    pub name: String,
    pub message: Option<String>,
}

/// Apply (create/update) a package from YAML content
#[allow(dead_code)]
pub async fn apply_package(
    yaml_content: &str,
) -> Result<PackageApplyResponse, anyhow::Error> {
    // Parse YAML to OPackage model for type-safe serialization
    let package: oprc_models::OPackage = serde_yaml::from_str(yaml_content)
        .map_err(|e| anyhow::anyhow!("Invalid YAML: {}", e))?;

    apply_package_struct(&package).await
}

/// Apply (create/update) a package from OPackage struct
pub async fn apply_package_struct(
    package: &oprc_models::OPackage,
) -> Result<PackageApplyResponse, anyhow::Error> {
    let json_body = serde_json::to_string(package)
        .map_err(|e| anyhow::anyhow!("JSON conversion failed: {}", e))?;

    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/packages", base);

    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(json_body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Package apply failed ({}): {}",
            status,
            body
        ));
    }

    // Try to parse response, fallback to generic success
    match resp.json::<PackageApplyResponse>().await {
        Ok(r) => Ok(r),
        Err(_) => Ok(PackageApplyResponse {
            name: package.name.clone(),
            message: Some("Package applied successfully".to_string()),
        }),
    }
}

/// Apply a package and optionally create deployments defined in it
/// This mirrors the CLI's `--apply-deployments` behavior
pub async fn apply_package_with_deployments(
    yaml_content: &str,
    also_deploy: bool,
) -> Result<PackageApplyResponse, anyhow::Error> {
    // Parse YAML to OPackage model
    let package: oprc_models::OPackage = serde_yaml::from_str(yaml_content)
        .map_err(|e| anyhow::anyhow!("Invalid YAML: {}", e))?;

    // Apply the package first
    let result = apply_package_struct(&package).await?;

    // If deployments flag is set and package has deployments, apply them
    if also_deploy && !package.deployments.is_empty() {
        let client = reqwest::Client::new();
        let base = crate::config::get_api_base_url();
        let url = format!("{}/api/v1/deployments", base);

        for mut dep in package.deployments.clone() {
            // Fill in package_name if empty (same as CLI behavior)
            if dep.package_name.is_empty() {
                dep.package_name = package.name.clone();
            }

            let json_body = serde_json::to_string(&dep).map_err(|e| {
                anyhow::anyhow!("JSON conversion failed: {}", e)
            })?;

            let resp = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(json_body)
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Deployment request failed: {}", e)
                })?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "Deployment '{}' apply failed ({}): {}",
                    dep.key,
                    status,
                    body
                ));
            }

            tracing::info!("Applied deployment '{}'", dep.key);
        }
    }

    Ok(result)
}

/// Delete a package by name
pub async fn delete_package(name: &str) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/packages/{}", base, name);

    let resp = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Package delete failed ({}): {}",
            status,
            body
        ));
    }

    Ok(())
}
