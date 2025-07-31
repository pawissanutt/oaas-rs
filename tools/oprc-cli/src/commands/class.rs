use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::{ClassOperation, OutputFormat};
use serde_json::Value;

/// Handle class management commands
pub async fn handle_class_command(
    operation: &ClassOperation,
) -> anyhow::Result<()> {
    match operation {
        ClassOperation::List { class_name } => {
            handle_class_list(class_name.as_deref()).await
        }
        ClassOperation::Delete { class_name } => {
            handle_class_delete(class_name).await
        }
    }
}

async fn handle_class_list(filter: Option<&str>) -> anyhow::Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    // Send request to list classes
    let path = if let Some(filter) = filter {
        format!("/classes?filter={}", filter)
    } else {
        "/classes".to_string()
    };

    let classes: Value = client.get(&path).await?;

    // Use the new output formatting system
    print_output(&classes, &OutputFormat::Json)?;

    Ok(())
}

async fn handle_class_delete(class_name: &str) -> anyhow::Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Deleting class: {}", class_name);

    // Send delete request
    let path = format!("/classes/{}", class_name);
    let _response: Value = client.delete(&path).await?;

    println!("Class '{}' deleted successfully", class_name);
    Ok(())
}
