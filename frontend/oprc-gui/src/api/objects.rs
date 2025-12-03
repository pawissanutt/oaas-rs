//! Object storage operations proxy

use crate::types::{
    ClassRuntime, ListObjectsResponse, ObjData, ObjectGetRequest,
    ObjectListItem, ObjectPutRequest,
};
use dioxus::prelude::*;

/// List available class runtimes from PM API (returns runtime info with partition_count)
pub async fn proxy_list_classes() -> Result<Vec<ClassRuntime>, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    // Use class-runtimes endpoint which returns deployed classes with partition_count
    let url = format!("{}/api/v1/class-runtimes", base);

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "List class runtimes failed: {}",
            resp.status()
        ));
    }

    resp.json::<Vec<ClassRuntime>>()
        .await
        .map_err(|e| anyhow::anyhow!("Parse error: {}", e))
}

/// List objects in a single partition (via PM proxy to Gateway)
pub async fn proxy_list_objects(
    class_key: &str,
    partition_id: u32,
    prefix: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> Result<ListObjectsResponse, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();

    // Use PM's gateway proxy: /api/gateway/* -> Gateway
    let mut url = format!(
        "{}/api/gateway/api/class/{}/{}/objects",
        base, class_key, partition_id
    );

    // Build query parameters
    let mut params = Vec::new();
    if let Some(p) = prefix {
        if !p.is_empty() {
            params.push(format!("prefix={}", p));
        }
    }
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    if let Some(c) = cursor {
        params.push(format!("cursor={}", c));
    }
    if !params.is_empty() {
        url = format!("{}?{}", url, params.join("&"));
    }

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("List objects failed: {}", resp.status()));
    }

    let mut response: ListObjectsResponse = resp
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

    // Add partition_id to each item
    for obj in &mut response.objects {
        obj.partition_id = partition_id;
    }

    Ok(response)
}

/// List objects across all partitions (parallel requests)
pub async fn proxy_list_objects_all_partitions(
    class_key: &str,
    partition_count: u32,
    prefix: Option<&str>,
    limit_per_partition: Option<u32>,
) -> Result<Vec<ObjectListItem>, anyhow::Error> {
    use futures::future::join_all;

    let futures: Vec<_> = (0..partition_count)
        .map(|pid| {
            let class_key = class_key.to_string();
            let prefix = prefix.map(|s| s.to_string());
            async move {
                proxy_list_objects(
                    &class_key,
                    pid,
                    prefix.as_deref(),
                    limit_per_partition,
                    None,
                )
                .await
            }
        })
        .collect();

    let results = join_all(futures).await;

    let mut all_objects = Vec::new();
    for result in results {
        match result {
            Ok(response) => all_objects.extend(response.objects),
            Err(e) => {
                // Log error but continue with other partitions
                tracing::warn!("Failed to list partition: {}", e);
            }
        }
    }

    Ok(all_objects)
}

pub async fn proxy_object_get(
    req: ObjectGetRequest,
) -> Result<ObjData, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    // Use PM's gateway proxy: /api/gateway/* -> Gateway
    let url = format!(
        "{}/api/gateway/api/class/{}/{}/objects/{}",
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
    // Use PM's gateway proxy: /api/gateway/* -> Gateway
    let url = format!(
        "{}/api/gateway/api/class/{}/{}/objects/{}",
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
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Object put failed: {}", body));
    }

    Ok(())
}

/// Delete an object
pub async fn proxy_object_delete(
    class_key: &str,
    partition_id: u32,
    object_id: &str,
) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!(
        "{}/api/gateway/api/class/{}/{}/objects/{}",
        base, class_key, partition_id, object_id
    );

    let resp = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Object delete failed: {}", body));
    }

    Ok(())
}

/// Get package by name (to retrieve class function bindings)
pub async fn proxy_get_package(
    package_name: &str,
) -> Result<oprc_models::OPackage, anyhow::Error> {
    let client = reqwest::Client::new();
    let base = crate::config::get_api_base_url();
    let url = format!("{}/api/v1/packages/{}", base, package_name);

    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Get package failed: {}", resp.status()));
    }

    resp.json::<oprc_models::OPackage>()
        .await
        .map_err(|e| anyhow::anyhow!("Parse error: {}", e))
}
