use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::{DeployOperation, OutputFormat};
use anyhow::Result;
use serde_json::Value;

/// Handle deployment management commands
pub async fn handle_deploy_command(operation: &DeployOperation) -> Result<()> {
    match operation {
        DeployOperation::List { name } => {
            handle_deploy_list(name.as_deref()).await
        }
        DeployOperation::Delete { name } => handle_deploy_delete(name).await,
    }
}

async fn handle_deploy_list(filter: Option<&str>) -> Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    // Build API path
    let path = if let Some(name) = filter {
        format!("/api/v1/deployments/{}", name)
    } else {
        "/api/v1/deployments".to_string()
    };

    // Fetch deployments
    let response: Value = client.get(&path).await?;

    // Print results using JSON format (can be enhanced later with OutputArgs)
    print_output(&response, &OutputFormat::Json)?;

    Ok(())
}

async fn handle_deploy_delete(name: &str) -> Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Deleting deployment: {}", name);

    // Send delete request
    let path = format!("/api/v1/deployments/{}", name);
    let _response: Value = client.delete(&path).await?;

    println!("Deployment '{}' deleted successfully", name);
    Ok(())
}
