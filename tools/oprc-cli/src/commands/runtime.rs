use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::{OutputFormat, RuntimeOperation};
use anyhow::Result;
use serde_json::Value;

/// Handle runtime management commands
pub async fn handle_runtime_command(
    operation: &RuntimeOperation,
) -> Result<()> {
    match operation {
        RuntimeOperation::List { name } => {
            handle_runtime_list(name.as_deref()).await
        }
        RuntimeOperation::Delete { name } => handle_runtime_delete(name).await,
    }
}

async fn handle_runtime_list(filter: Option<&str>) -> Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    // Build API path
    let path = if let Some(name) = filter {
        format!("/class-runtimes/{}", name)
    } else {
        "/class-runtimes".to_string()
    };

    // Fetch class runtimes
    let response: Value = client.get(&path).await?;

    // Print results using JSON format (can be enhanced later with OutputArgs)
    print_output(&response, &OutputFormat::Json)?;

    Ok(())
}

async fn handle_runtime_delete(name: &str) -> Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Deleting class runtime: {}", name);

    // Send delete request
    let path = format!("/class-runtimes/{}", name);
    let _response: Value = client.delete(&path).await?;

    println!("Class runtime '{}' deleted successfully", name);
    Ok(())
}
