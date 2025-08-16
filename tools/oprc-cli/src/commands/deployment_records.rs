use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::OutputFormat;
use anyhow::Result;
use serde_json::Value;

/// List deployment records or fetch a specific record by ID
pub async fn handle_deployment_records_command(id: &Option<String>) -> Result<()> {
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;
    let client = HttpClient::new(context)?;
    let path = if let Some(id) = id { format!("/api/v1/deployment-records/{id}") } else { "/api/v1/deployment-records".to_string() };
    let response: Value = client.get(&path).await?;
    print_output(&response, &OutputFormat::Json)?;
    Ok(())
}
