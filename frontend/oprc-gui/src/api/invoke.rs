//! Function invocation proxy

use crate::types::{InvocationResponse, InvokeRequest};
use dioxus::prelude::*;

#[post("/api/proxy/invoke")]
pub async fn proxy_invoke(
    req: InvokeRequest,
) -> Result<InvocationResponse, ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        // This code is never reached on client - Dioxus replaces the entire function body
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            // Mock response using real InvocationResponse type
            use oprc_grpc::ResponseStatus;
            Ok(InvocationResponse {
                payload: Some(
                    serde_json::to_vec(&serde_json::json!({
                        "mock": true,
                        "message": "Mock invoke result",
                        "input": req.payload
                    }))
                    .unwrap(),
                ),
                status: ResponseStatus::Okay as i32,
                headers: std::collections::HashMap::from([(
                    "content-type".to_string(),
                    "application/json".to_string(),
                )]),
                invocation_id: "mock-invocation-id".to_string(),
            })
        } else {
            // Relay to gateway
            let url = if let Some(ref oid) = req.object_id {
                // Stateful: POST /api/class/<class>/<partition>/objects/<object_id>/invokes/<method_id>
                format!(
                    "{}/api/class/{}/{}/objects/{}/invokes/{}",
                    crate::config::gateway_base_url(),
                    req.class_key,
                    req.partition_id,
                    oid,
                    req.function_key
                )
            } else {
                // Stateless: POST /api/class/<class>/<partition>/invokes/<method_id>
                format!(
                    "{}/api/class/{}/{}/invokes/{}",
                    crate::config::gateway_base_url(),
                    req.class_key,
                    req.partition_id,
                    req.function_key
                )
            };

            let client = reqwest::Client::new();
            let resp = client
                .post(&url)
                .header("Accept", "application/json")
                .json(&req.payload)
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            // Gateway returns raw response, wrap in InvocationResponse
            let payload = resp
                .bytes()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?
                .to_vec();

            use oprc_grpc::ResponseStatus;
            Ok(InvocationResponse {
                payload: Some(payload),
                status: ResponseStatus::Okay as i32,
                headers: std::collections::HashMap::new(),
                invocation_id: "".to_string(),
            })
        }
    }
}
