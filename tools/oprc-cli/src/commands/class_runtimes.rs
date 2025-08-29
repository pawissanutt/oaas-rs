use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::OutputFormat;
use anyhow::Result;
use serde_json::Value;

/// List class runtimes or fetch a specific runtime by ID
pub async fn handle_class_runtimes_command(id: &Option<String>) -> Result<()> {
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;
    let client = HttpClient::new(context)?;
    let path = if let Some(id) = id {
        format!("/api/v1/class-runtimes/{id}")
    } else {
        "/api/v1/class-runtimes".to_string()
    };
    let response: Value = client.get(&path).await?;
    print_output(&response, &OutputFormat::Json)?;
    Ok(())
}
