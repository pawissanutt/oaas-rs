//! Deployments management proxy

use dioxus::prelude::*;
use oprc_models::OClassDeployment;

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
