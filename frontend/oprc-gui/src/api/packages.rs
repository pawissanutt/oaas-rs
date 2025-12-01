//! Packages listing

use crate::types::PackagesSnapshot;
use dioxus::prelude::*;

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
