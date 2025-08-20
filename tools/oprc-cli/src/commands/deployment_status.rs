use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::OutputFormat;
use anyhow::Result;
use serde_json::Value;

/// Fetch deployment status by ID
pub async fn handle_deployment_status_command(id: &String) -> Result<()> {
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;
    let client = HttpClient::new(context)?;
    let path = format!("/api/v1/deployment-status/{id}");
    let response: Value = client.get(&path).await?;
    print_output(&response, &OutputFormat::Json)?;
    Ok(())
}
