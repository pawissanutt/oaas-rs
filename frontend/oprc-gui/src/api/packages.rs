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
pub async fn apply_package(yaml_content: &str) -> Result<PackageApplyResponse, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/packages", base);

    let resp = client
        .post(&url)
        .header("Content-Type", "application/x-yaml")
        .body(yaml_content.to_string())
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
            name: "unknown".to_string(),
            message: Some("Package applied successfully".to_string()),
        }),
    }
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
