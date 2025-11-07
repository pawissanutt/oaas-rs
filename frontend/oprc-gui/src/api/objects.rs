//! Object storage operations proxy

use crate::types::{
    ObjData, ObjMeta, ObjectGetRequest, ObjectPutRequest, ValData,
};
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
            // Mock object data using real ObjData type
            let object_id = req.object_id.parse::<u64>().unwrap_or(0);
            Ok(ObjData {
                metadata: Some(ObjMeta {
                    cls_id: req.class_key.clone(),
                    partition_id: req.partition_id.parse().unwrap_or(0),
                    object_id,
                    object_id_str: if object_id == 0 {
                        Some(req.object_id.clone())
                    } else {
                        None
                    },
                }),
                entries: std::collections::HashMap::from([(
                    42,
                    ValData {
                        data: serde_json::to_vec(&serde_json::json!({
                            "counter": 42,
                            "name": "Mock Object",
                            "created_at": "2025-11-07T12:00:00Z"
                        }))
                        .unwrap(),
                        r#type: 0, // VAL_TYPE_BYTE
                    },
                )]),
                entries_str: std::collections::HashMap::new(),
                event: None,
            })
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

            // Gateway returns ObjData as JSON when Accept: application/json
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
            // Mock: just succeed
            Ok(())
        } else {
            // Relay to gateway: PUT /api/class/{}/{}/objects/{}
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
