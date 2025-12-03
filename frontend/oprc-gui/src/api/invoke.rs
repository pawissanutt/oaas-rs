//! Function invocation proxy

use crate::types::{InvocationResponse, InvokeRequest};
use dioxus::prelude::*;

pub async fn proxy_invoke(
    req: InvokeRequest,
) -> Result<InvocationResponse, anyhow::Error> {
    let client = reqwest::Client::new();

    // Construct URL using PM's gateway proxy: /api/gateway/* -> Gateway
    let base = crate::config::get_api_base_url();
    let url = if let Some(ref oid) = req.object_id {
        // Stateful invocation (on specific object)
        format!(
            "{}/api/gateway/api/class/{}/{}/objects/{}/invokes/{}",
            base, req.class_key, req.partition_id, oid, req.function_key
        )
    } else {
        // Stateless invocation
        format!(
            "{}/api/gateway/api/class/{}/{}/invokes/{}",
            base, req.class_key, req.partition_id, req.function_key
        )
    };

    let resp = client
        .post(&url)
        .header("Accept", "application/json")
        .json(&req.payload)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Invoke failed: {}", resp.status()));
    }

    let payload = resp.bytes().await.map_err(|e| anyhow::anyhow!(e))?.to_vec();

    // We need to construct InvocationResponse.
    // The Gateway returns the raw bytes of the function result.

    Ok(InvocationResponse {
        payload: Some(payload),
        status: 200, // Assuming success if we got here
        headers: std::collections::HashMap::new(),
        invocation_id: "".to_string(),
    })
}
