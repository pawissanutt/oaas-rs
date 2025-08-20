use crate::client::HttpClient;
use crate::config::ContextManager;
use crate::types::PackageOperation;
use oprc_models::enums::{FunctionAccessModifier, FunctionType};
use oprc_models::package::{
    FunctionBinding, OClass, OFunction, OPackage, PackageMetadata,
};
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

    // Build OFunction list (flatten unique functions across classes)
    let mut functions: Vec<OFunction> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for class_def in &package_def.classes {
        for f in &class_def.functions {
            if seen.insert(f.function.clone()) {
                functions.push(OFunction {
                    key: f.function.clone(),
                    function_type: FunctionType::Custom,
                    description: None,
                    provision_config: Some(oprc_models::nfr::ProvisionConfig {
                        container_image: Some(f.code.clone()),
                        ..Default::default()
                    }),
                    config: std::collections::HashMap::new(),
                });
            }
        }
    }

    // Build classes with bindings to functions defined under them
    let classes: Vec<OClass> = package_def
        .classes
        .iter()
        .map(|c| OClass {
            key: c.class.clone(),
            description: None,
            state_spec: None,
            function_bindings: c
                .functions
                .iter()
                .map(|f| FunctionBinding {
                    name: f.function.clone(),
                    function_key: f.function.clone(),
                    access_modifier: FunctionAccessModifier::Public,
                    immutable: false,
                    parameters: vec![],
                })
                .collect(),
            disabled: false,
        })
        .collect();

    let pkg = OPackage {
        name: package_def.package.clone(),
        version: Some(package_def.version.clone()),
        disabled: false,
        metadata: PackageMetadata {
            author: None,
            description: None,
            tags: vec![],
            created_at: None,
            updated_at: None,
        },
        classes,
        functions,
        dependencies: vec![],
        deployments: vec![],
    };

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
    let path = format!("/api/v1/packages/{}", package_def.package);
    let _response: serde_json::Value = client.delete(&path).await?;

    println!("Package '{}' deleted successfully", package_def.package);
    Ok(())
}
