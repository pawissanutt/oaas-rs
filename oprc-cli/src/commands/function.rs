use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::types::FunctionOperation;
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

    println!("Fetching function list...");

    // Send request to list functions
    let path = if let Some(filter) = filter {
        format!("/functions?filter={}", filter)
    } else {
        "/functions".to_string()
    };

    let functions: Value = client.get(&path).await?;

    // Format and display results
    if let Some(function_array) = functions.as_array() {
        if function_array.is_empty() {
            println!("No functions found");
        } else {
            println!("Functions:");
            for function in function_array {
                if let Some(function_name) =
                    function.get("name").and_then(|n| n.as_str())
                {
                    let class = function
                        .get("class")
                        .and_then(|c| c.as_str())
                        .unwrap_or("unknown");
                    let package = function
                        .get("package")
                        .and_then(|p| p.as_str())
                        .unwrap_or("unknown");

                    println!("  {}.{}.{}", package, class, function_name);

                    // Show function details if available
                    if let Some(params) =
                        function.get("parameters").and_then(|p| p.as_array())
                    {
                        println!(
                            "    Parameters: {}",
                            params
                                .iter()
                                .map(|p| p.as_str().unwrap_or("?"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
            }
        }
    } else {
        println!("Unexpected response format: {}", functions);
    }

    Ok(())
}
