use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::types::PackageOperation;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize)]
struct PackageDefinition {
    package: String,
    version: String,
    classes: Vec<ClassDefinition>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClassDefinition {
    class: String,
    functions: Vec<FunctionDefinition>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FunctionDefinition {
    function: String,
    code: String,
}

pub async fn handle_package_command(
    operation: &PackageOperation,
) -> anyhow::Result<()> {
    match operation {
        PackageOperation::Apply {
            file,
            override_package,
        } => handle_package_apply(file, override_package.as_ref()).await,
        PackageOperation::Delete {
            file,
            override_package,
        } => handle_package_delete(file, override_package.as_ref()).await,
    }
}

async fn handle_package_apply(
    file: &PathBuf,
    override_package: Option<&String>,
) -> anyhow::Result<()> {
    // Load and validate context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Check if file exists
    if !file.exists() {
        return Err(anyhow::anyhow!(
            "Package file '{}' not found",
            file.display()
        ));
    }

    // Read and parse YAML file
    let content = fs::read_to_string(&file).await?;
    let mut package_def: PackageDefinition = serde_yaml::from_str(&content)?;

    // Override package name if provided
    if let Some(override_name) = override_package {
        package_def.package = override_name.clone();
    }

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!(
        "Applying package: {} v{}",
        package_def.package, package_def.version
    );

    // Apply each class in the package
    for class_def in &package_def.classes {
        println!("  Creating class: {}", class_def.class);

        // Apply each function in the class
        for func_def in &class_def.functions {
            println!("    Adding function: {}", func_def.function);

            // Prepare function payload
            let payload = serde_json::json!({
                "package": package_def.package,
                "class": class_def.class,
                "function": func_def.function,
                "code": func_def.code
            });

            // Send to package manager
            let path =
                format!("/functions/{}/{}", class_def.class, func_def.function);
            let _response: serde_json::Value =
                client.post(&path, &payload).await?;
        }
    }

    println!("Package '{}' applied successfully", package_def.package);
    Ok(())
}

async fn handle_package_delete(
    file: &PathBuf,
    override_package: Option<&String>,
) -> anyhow::Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Check if file exists
    if !file.exists() {
        return Err(anyhow::anyhow!(
            "Package file '{}' not found",
            file.display()
        ));
    }

    // Read and parse YAML file to get package name
    let content = fs::read_to_string(&file).await?;
    let mut package_def: PackageDefinition = serde_yaml::from_str(&content)?;

    // Override package name if provided
    if let Some(override_name) = override_package {
        package_def.package = override_name.clone();
    }

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Deleting package: {}", package_def.package);

    // Send delete request
    let path = format!("/packages/{}", package_def.package);
    let _response: serde_json::Value = client.delete(&path).await?;

    println!("Package '{}' deleted successfully", package_def.package);
    Ok(())
}
