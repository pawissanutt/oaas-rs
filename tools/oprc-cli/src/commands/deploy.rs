use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::output::print_output;
use crate::types::{DeployOperation, OutputFormat};
use anyhow::Result;
use serde_json::Value;

/// Handle deployment management commands
pub async fn handle_deploy_command(operation: &DeployOperation) -> Result<()> {
    match operation {
        DeployOperation::List { name } => {
            handle_deploy_list(name.as_deref()).await
        }
        DeployOperation::Apply {
            file,
            override_package,
            overwrite,
        } => {
            handle_deploy_apply(file, override_package.as_ref(), *overwrite)
                .await
        }
        DeployOperation::Delete { name } => handle_deploy_delete(name).await,
    }
}

async fn handle_deploy_list(filter: Option<&str>) -> Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    // Build API path
    let path = if let Some(name) = filter {
        format!("/api/v1/deployments/{}", name)
    } else {
        "/api/v1/deployments".to_string()
    };

    // Fetch deployments
    let response: Value = client.get(&path).await?;

    // Print results using JSON format (can be enhanced later with OutputArgs)
    print_output(&response, &OutputFormat::Json)?;

    Ok(())
}

async fn handle_deploy_delete(name: &str) -> Result<()> {
    // Load context
    let manager = ContextManager::new().await?;
    let context = manager
        .get_current_context()
        .ok_or_else(|| anyhow::anyhow!("No context selected"))?;

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Deleting deployment: {}", name);

    // Send delete request
    let path = format!("/api/v1/deployments/{}", name);
    let _response: Value = client.delete(&path).await?;

    println!("Deployment '{}' deleted successfully", name);
    Ok(())
}

async fn handle_deploy_apply(
    file: &std::path::PathBuf,
    override_package: Option<&String>,
    overwrite: bool,
) -> Result<()> {
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

    // Read and parse YAML file into OPackage to extract deployments
    let content = tokio::fs::read_to_string(&file).await?;
    let mut pkg: oprc_models::package::OPackage =
        serde_yaml::from_str(&content)?;

    // Override package name if provided
    if let Some(name) = override_package {
        pkg.name = name.clone();
    }

    if pkg.deployments.is_empty() {
        println!(
            "No deployments found in package '{}'. Nothing to apply.",
            pkg.name
        );
        return Ok(());
    }

    // Create HTTP client
    let client = HttpClient::new(context)?;

    // Post each deployment to PM
    for mut dep in pkg.deployments.clone() {
        // Ensure package_name is set; if empty, fill with pkg.name
        if dep.package_name.is_empty() {
            dep.package_name = pkg.name.clone();
        }

        // Basic info log
        println!(
            "Applying deployment: key='{}' package='{}' class='{}' envs={:?}",
            dep.key, dep.package_name, dep.class_key, dep.target_envs
        );

        // Create or update based on flag and existence
        // check existence via GET /deployments/{key}
        let get_path = format!("/api/v1/deployments/{}", dep.key);
        let exists = match client.get::<serde_json::Value>(&get_path).await {
            Ok(_) => true,
            Err(e) => {
                let msg = format!("{e:?}");
                if msg.contains("404") {
                    false
                } else {
                    return Err(e.into());
                }
            }
        };

        if exists {
            if overwrite {
                // PM doesn't expose a dedicated update endpoint; recreate to update
                let _resp: serde_json::Value =
                    client.post("/api/v1/deployments", &dep).await?;
                println!("Updated deployment '{}' successfully", dep.key);
            } else {
                return Err(anyhow::anyhow!(
                    "Deployment '{}' already exists (use --overwrite to replace)",
                    dep.key
                ));
            }
        } else {
            let _resp: serde_json::Value =
                client.post("/api/v1/deployments", &dep).await?;
            println!("Created deployment '{}' successfully", dep.key);
        }
    }

    println!(
        "All deployments from package '{}' applied successfully",
        pkg.name
    );

    Ok(())
}
