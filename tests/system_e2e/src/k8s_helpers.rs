// Kubernetes helpers for E2E tests

use anyhow::{Context, Result, bail};
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client, api::ListParams};
use std::time::Duration;

/// Wait for pods in a namespace to be ready
pub async fn wait_for_pods_ready(
    client: &Client,
    namespace: &str,
    label_selector: Option<&str>,
    min_pods: usize,
    timeout_secs: u64,
) -> Result<()> {
    tracing::info!(
        "Waiting for at least {} pods in namespace {} to be ready",
        min_pods,
        namespace
    );
    
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    let mut lp = ListParams::default();
    if let Some(selector) = label_selector {
        lp = lp.labels(selector);
    }
    
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            bail!(
                "Timeout waiting for pods in namespace {} to be ready",
                namespace
            );
        }
        
        let pod_list = pods.list(&lp).await.context("Failed to list pods")?;
        
        let ready_count = pod_list
            .items
            .iter()
            .filter(|pod| is_pod_ready(pod))
            .count();
        
        tracing::debug!(
            "Found {} ready pods (need {})",
            ready_count,
            min_pods
        );
        
        if ready_count >= min_pods {
            tracing::info!(
                "Found {} ready pods in namespace {}",
                ready_count,
                namespace
            );
            return Ok(());
        }
        
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Check if a pod is ready
fn is_pod_ready(pod: &Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .map(|conditions| {
            conditions
                .iter()
                .any(|c| c.type_ == "Ready" && c.status == "True")
        })
        .unwrap_or(false)
}

/// Wait for specific pods by name pattern to be ready
pub async fn wait_for_named_pods_ready(
    client: &Client,
    namespace: &str,
    name_prefix: &str,
    min_pods: usize,
    timeout_secs: u64,
) -> Result<()> {
    tracing::info!(
        "Waiting for pods with prefix '{}' in namespace {} to be ready",
        name_prefix,
        namespace
    );
    
    let pods: Api<Pod> = Api::namespaced(client.clone(), namespace);
    
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            bail!(
                "Timeout waiting for pods with prefix '{}' in namespace {} to be ready",
                name_prefix,
                namespace
            );
        }
        
        let pod_list = pods.list(&ListParams::default()).await?;
        
        let ready_count = pod_list
            .items
            .iter()
            .filter(|pod| {
                pod.metadata
                    .name
                    .as_ref()
                    .map(|n| n.starts_with(name_prefix))
                    .unwrap_or(false)
                    && is_pod_ready(pod)
            })
            .count();
        
        tracing::debug!(
            "Found {} ready pods with prefix '{}' (need {})",
            ready_count,
            name_prefix,
            min_pods
        );
        
        if ready_count >= min_pods {
            tracing::info!(
                "Found {} ready pods with prefix '{}'",
                ready_count,
                name_prefix
            );
            return Ok(());
        }
        
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Wait for OaaS control plane to be ready
pub async fn wait_for_oaas_control_plane(
    client: &Client,
    namespace: &str,
    timeout_secs: u64,
) -> Result<()> {
    tracing::info!("Waiting for OaaS control plane to be ready");
    
    // Wait for PM pods
    wait_for_named_pods_ready(client, namespace, "oaas-pm", 1, timeout_secs).await?;
    
    // Wait for CRM pods  
    wait_for_named_pods_ready(client, namespace, "oaas-crm", 1, timeout_secs).await?;
    
    // Wait for Gateway pods
    wait_for_named_pods_ready(client, namespace, "oaas-gateway", 1, timeout_secs).await?;
    
    tracing::info!("OaaS control plane is ready");
    Ok(())
}

/// Get service endpoint URL
pub async fn get_service_url(
    client: &Client,
    namespace: &str,
    service_name: &str,
    port: u16,
) -> Result<String> {
    use k8s_openapi::api::core::v1::Service;
    
    let services: Api<Service> = Api::namespaced(client.clone(), namespace);
    let _svc = services
        .get(service_name)
        .await
        .context(format!("Failed to get service {}", service_name))?;
    
    // For Kind, we typically use port-forward or NodePort
    // For simplicity, return localhost with the specified port
    // In a real scenario, you might need to set up port-forwarding
    Ok(format!("http://localhost:{}", port))
}

/// Check if namespace exists
pub async fn namespace_exists(client: &Client, namespace: &str) -> Result<bool> {
    use k8s_openapi::api::core::v1::Namespace;
    
    let namespaces: Api<Namespace> = Api::all(client.clone());
    Ok(namespaces.get_opt(namespace).await?.is_some())
}

/// Wait for ODGM pods to appear for a specific deployment
pub async fn wait_for_odgm_pods(
    client: &Client,
    namespace: &str,
    class_key: &str,
    min_pods: usize,
    timeout_secs: u64,
) -> Result<()> {
    tracing::info!(
        "Waiting for ODGM pods for class {} in namespace {}",
        class_key,
        namespace
    );
    
    // ODGM pods typically have a label like "oaas.io/class=<class_key>"
    let label_selector = format!("oaas.io/class={}", class_key);
    
    wait_for_pods_ready(
        client,
        namespace,
        Some(&label_selector),
        min_pods,
        timeout_secs,
    )
    .await
}
