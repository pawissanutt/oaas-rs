use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::{FunctionOperation, OutputFormat};
use serde_json::Value;

/// Handle function management commands
pub async fn handle_function_command(
    operation: &FunctionOperation,
) -> anyhow::Result<()> {
    match operation {
        FunctionOperation::List { function_name } => {
            handle_function_list(function_name.as_deref()).await
        }
    }
}

async fn handle_function_list(filter: Option<&str>) -> anyhow::Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    // Send request to list functions
    let path = if let Some(filter) = filter {
        format!("/api/v1/functions?filter={}", filter)
    } else {
        "/api/v1/functions".to_string()
    };

    let functions: Value = client.get(&path).await?;

    // Use the new output formatting system
    print_output(&functions, &OutputFormat::Json)?;

    Ok(())
}
