//! Object storage operations proxy

use crate::types::{ObjData, ObjectGetRequest, ObjectPutRequest};
use dioxus::prelude::*;

pub async fn proxy_object_get(
    req: ObjectGetRequest,
) -> Result<ObjData, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!(
        "{}/api/class/{}/{}/objects/{}",
        base, req.class_key, req.partition_id, req.object_id
    );

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Object get failed: {}", resp.status()));
    }

    resp.json::<ObjData>().await.map_err(|e| anyhow::anyhow!(e))
}

pub async fn proxy_object_put(
    req: ObjectPutRequest,
) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!(
        "{}/api/class/{}/{}/objects/{}",
        base, req.class_key, req.partition_id, req.object_id
    );

    let resp = client
        .put(&url)
        .header("Content-Type", "application/json")
        .json(&req.data)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Object put failed: {}", resp.status()));
    }

    Ok(())
}
