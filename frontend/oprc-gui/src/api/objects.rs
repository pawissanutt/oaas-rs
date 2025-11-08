//! Object storage operations proxy (gateway relay or mock)

use crate::types::{ObjData, ObjectGetRequest, ObjectPutRequest};
use dioxus::prelude::*;

#[post("/api/proxy/object_get")]
pub async fn proxy_object_get(
    req: ObjectGetRequest,
) -> Result<ObjData, ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            Ok(crate::api::mock::mock_object_data(&req))
        } else {
            // Relay to gateway: GET /api/class/<class>/<partition>/objects/<object_id>
            let url = format!(
                "{}/api/class/{}/{}/objects/{}",
                crate::config::gateway_base_url(),
                req.class_key,
                req.partition_id,
                req.object_id
            );
            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .header("Accept", "application/json")
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            resp.json::<ObjData>()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))
        }
    }
}

#[post("/api/proxy/object_put")]
pub async fn proxy_object_put(
    req: ObjectPutRequest,
) -> Result<(), ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            // Accept silently in mock mode
            Ok(())
        } else {
            // Relay to gateway: PUT /api/class/<class>/<partition>/objects/<object_id>
            let url = format!(
                "{}/api/class/{}/{}/objects/{}",
                crate::config::gateway_base_url(),
                req.class_key,
                req.partition_id,
                req.object_id
            );
            let client = reqwest::Client::new();
            client
                .put(&url)
                .header("Content-Type", "application/json")
                .json(&req.data)
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            Ok(())
        }
    }
}
