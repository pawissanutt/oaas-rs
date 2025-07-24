use crate::config::ContextManager;
use crate::client::HttpClient;
use crate::types::ClassOperation;
use serde_json::Value;

/// Handle class management commands
pub async fn handle_class_command(operation: &ClassOperation) -> anyhow::Result<()> {
    match operation {
        ClassOperation::List { class_name } => handle_class_list(class_name.as_deref()).await,
        ClassOperation::Delete { class_name } => handle_class_delete(class_name).await,
    }
}

async fn handle_class_list(filter: Option<&str>) -> anyhow::Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager.get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Fetching class list...");

    // Send request to list classes
    let path = if let Some(filter) = filter {
        format!("/classes?filter={}", filter)
    } else {
        "/classes".to_string()
    };
    
    let classes: Value = client.get(&path).await?;

    // Format and display results
    if let Some(class_array) = classes.as_array() {
        if class_array.is_empty() {
            println!("No classes found");
        } else {
            println!("Classes:");
            for class in class_array {
                if let Some(class_name) = class.get("name").and_then(|n| n.as_str()) {
                    let package = class.get("package")
                        .and_then(|p| p.as_str())
                        .unwrap_or("unknown");
                    let functions = class.get("functions")
                        .and_then(|f| f.as_array())
                        .map(|f| f.len())
                        .unwrap_or(0);
                    
                    println!("  {}.{} ({} functions)", package, class_name, functions);
                }
            }
        }
    } else {
        println!("Unexpected response format: {}", classes);
    }

    Ok(())
}

async fn handle_class_delete(class_name: &str) -> anyhow::Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager.get_current_context()
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
