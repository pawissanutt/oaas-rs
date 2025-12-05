//! Deployments management proxy

use dioxus::prelude::*;
use oprc_models::OClassDeployment;
use serde::{Deserialize, Serialize};

pub async fn proxy_deployments() -> Result<Vec<OClassDeployment>, anyhow::Error>
{
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/deployments", base);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("API error: {}", resp.status()));
    }
    resp.json().await.map_err(|e| anyhow::anyhow!(e))
}

/// Response from apply deployment endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyDeploymentResponse {
    pub message: Option<String>,
    pub deployment_key: Option<String>,
}

/// Apply (create/update) a deployment from YAML content
pub async fn apply_deployment(yaml_content: &str) -> Result<ApplyDeploymentResponse, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/deployments", base);
    
    let resp = client
        .post(&url)
        .header("Content-Type", "application/yaml")
        .body(yaml_content.to_string())
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("API error {}: {}", status, body));
    }
    
    resp.json().await.map_err(|e| anyhow::anyhow!(e))
}

/// Delete a deployment by key
pub async fn delete_deployment(key: &str) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/deployments/{}", base, key);
    
    let resp = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Delete failed {}: {}", status, body));
    }
    
    Ok(())
}
