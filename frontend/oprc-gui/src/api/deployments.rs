//! Deployments management proxy

use dioxus::prelude::*;
use oprc_models::OClassDeployment;

#[post("/api/proxy/deployments")]
pub async fn proxy_deployments() -> Result<Vec<OClassDeployment>, ServerFnError>
{
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            Ok(crate::api::mock::mock_deployments())
        } else {
            // Relay to PM: GET /api/v1/deployments
            let url =
                format!("{}/api/v1/deployments", crate::config::pm_base_url());
            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            resp.json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))
        }
    }
}
