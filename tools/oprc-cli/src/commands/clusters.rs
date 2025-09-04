use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::OutputFormat;
use anyhow::Result;
use serde_json::Value;

/// List known environments (clusters) from the Package Manager
pub async fn handle_envs_command() -> Result<()> {
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;
    let client = HttpClient::new(context)?;
    let response: Value = client.get("/api/v1/envs").await?;
    print_output(&response, &OutputFormat::Json)?;
    Ok(())
}
