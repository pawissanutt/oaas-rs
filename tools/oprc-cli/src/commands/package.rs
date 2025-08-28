use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::types::PackageOperation;
use oprc_models::package::OPackage;
use std::path::PathBuf;
use tokio::fs;

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

    // Read and parse YAML file directly into OPackage
    let content = fs::read_to_string(&file).await?;
    let mut pkg: OPackage = serde_yaml::from_str(&content)?;

    // Override package name if provided
    if let Some(override_name) = override_package {
        pkg.name = override_name.clone();
    }

    // Create HTTP client
    let client = HttpClient::new(context)?;

    let version_display = pkg
        .version
        .as_ref()
        .map(|v| format!(" v{}", v))
        .unwrap_or_default();
    println!("Applying package: {}{}", pkg.name, version_display);

    // Determine create vs update: try GET /packages/{name}
    let get_path = format!("/api/v1/packages/{}", pkg.name);
    let exists = match client.get::<serde_json::Value>(&get_path).await {
        Ok(_) => true,
        Err(e) => {
            // treat 404 as not exists; other errors bubble
            let msg = format!("{e:?}");
            if msg.contains("404") {
                false
            } else {
                return Err(e.into());
            }
        }
    };

    if exists {
        let path = format!("/api/v1/packages/{}", pkg.name);
        let _resp: serde_json::Value = client.post(&path, &pkg).await?; // PM uses POST for update
        println!("Updated package '{}'.", pkg.name);
    } else {
        let _resp: serde_json::Value =
            client.post("/api/v1/packages", &pkg).await?;
        println!("Created package '{}'.", pkg.name);
    }

    println!("Package '{}' applied successfully", pkg.name);
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

    // Read and parse YAML file to get package (and thus name)
    let content = fs::read_to_string(&file).await?;
    let mut pkg: OPackage = serde_yaml::from_str(&content)?;

    // Override package name if provided
    if let Some(override_name) = override_package {
        pkg.name = override_name.clone();
    }

    // Create HTTP client
    let client = HttpClient::new(context)?;

    println!("Deleting package: {}", pkg.name);

    // Send delete request
    let path = format!("/api/v1/packages/{}", pkg.name);
    let _response: serde_json::Value = client.delete(&path).await?;

    println!("Package '{}' deleted successfully", pkg.name);
    Ok(())
}
