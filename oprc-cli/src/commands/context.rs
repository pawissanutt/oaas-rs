use crate::config::ContextManager;
use crate::types::ContextOperation;
use anyhow::Result;
use serde_json;

/// Handle context management commands
pub async fn handle_context_command(
    operation: &ContextOperation,
) -> Result<()> {
    match operation {
        ContextOperation::Set {
            name,
            pm,
            gateway,
            cls,
            zenoh_peer,
        } => {
            handle_context_set(
                name.clone(),
                pm.clone(),
                gateway.clone(),
                cls.clone(),
                zenoh_peer.clone(),
            )
            .await
        }
        ContextOperation::Get => handle_context_get().await,
        ContextOperation::Select { name } => {
            handle_context_select(name.clone()).await
        }
    }
}
#[allow(dead_code)]
/// Handle context management commands with a specific ContextManager (useful for testing)
pub async fn handle_context_command_with_manager(
    operation: &ContextOperation,
    manager: &mut ContextManager,
) -> Result<()> {
    match operation {
        ContextOperation::Set {
            name,
            pm,
            gateway,
            cls,
            zenoh_peer,
        } => {
            handle_context_set_with_manager(
                name.clone(),
                pm.clone(),
                gateway.clone(),
                cls.clone(),
                zenoh_peer.clone(),
                manager,
            )
            .await
        }
        ContextOperation::Get => handle_context_get_with_manager(manager).await,
        ContextOperation::Select { name } => {
            handle_context_select_with_manager(name.clone(), manager).await
        }
    }
}

/// Handle context set command
async fn handle_context_set(
    name: Option<String>,
    pm_url: Option<String>,
    gateway_url: Option<String>,
    default_class: Option<String>,
    zenoh_peer: Option<String>,
) -> Result<()> {
    let mut context_manager = ContextManager::new().await?;

    context_manager
        .set_context(
            name.clone(),
            pm_url,
            gateway_url,
            default_class,
            zenoh_peer,
        )
        .await?;

    let context_name = name
        .unwrap_or_else(|| context_manager.config().current_context.clone());
    println!("ctx:'{}' updated successfully", context_name);

    // Show current configuration
    if let Some(context) = context_manager.config().get_context(&context_name) {
        println!("Configuration:");
        if let Some(pm) = &context.pm_url {
            println!("  pmUrl: '{}'", pm);
        }
        if let Some(gateway) = &context.gateway_url {
            println!("  gatewayUrl: '{}'", gateway);
        }
        if let Some(class) = &context.default_class {
            println!("  defaultClass: '{}'", class);
        }
        if let Some(peer) = &context.zenoh_peer {
            println!("  zenohPeer: '{}'", peer);
        }
    }

    Ok(())
}

/// Handle context get command
async fn handle_context_get() -> Result<()> {
    let context_manager = ContextManager::new().await?;

    // Pretty print the entire configuration
    let config_json = serde_json::to_string_pretty(context_manager.config())?;
    println!("{}", config_json);

    Ok(())
}

/// Handle context select command
async fn handle_context_select(name: String) -> Result<()> {
    let mut context_manager = ContextManager::new().await?;

    // Check if context exists
    if context_manager.config().get_context(&name).is_none() {
        return Err(anyhow::anyhow!("Context '{}' does not exist", name));
    }

    context_manager.select_context(name.clone()).await?;
    println!("Switched to context '{}'", name);

    Ok(())
}
#[allow(dead_code)]
/// Handle context set command with provided manager
async fn handle_context_set_with_manager(
    name: Option<String>,
    pm_url: Option<String>,
    gateway_url: Option<String>,
    default_class: Option<String>,
    zenoh_peer: Option<String>,
    manager: &mut ContextManager,
) -> Result<()> {
    manager
        .set_context(
            name.clone(),
            pm_url,
            gateway_url,
            default_class,
            zenoh_peer,
        )
        .await?;

    let context_name =
        name.unwrap_or_else(|| manager.config().current_context.clone());
    println!("ctx:'{}' updated successfully", context_name);

    // Show current configuration
    if let Some(context) = manager.config().get_context(&context_name) {
        println!("Configuration:");
        if let Some(pm) = &context.pm_url {
            println!("  pmUrl: '{}'", pm);
        }
        if let Some(gateway) = &context.gateway_url {
            println!("  gatewayUrl: '{}'", gateway);
        }
        if let Some(class) = &context.default_class {
            println!("  defaultClass: '{}'", class);
        }
        if let Some(peer) = &context.zenoh_peer {
            println!("  zenohPeer: '{}'", peer);
        }
    }

    Ok(())
}
#[allow(dead_code)]
/// Handle context get command with provided manager
async fn handle_context_get_with_manager(
    manager: &ContextManager,
) -> Result<()> {
    let current_context = &manager.config().current_context;

    if let Some(context) = manager.config().get_context(current_context) {
        println!("Current context: {}", current_context);

        let json_output = serde_json::json!({
            "pmUrl": context.pm_url,
            "gatewayUrl": context.gateway_url,
            "defaultClass": context.default_class,
            "zenohPeer": context.zenoh_peer
        });

        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!("No current context set");
    }

    Ok(())
}
#[allow(dead_code)]
/// Handle context select command with provided manager
async fn handle_context_select_with_manager(
    name: String,
    manager: &mut ContextManager,
) -> Result<()> {
    // Check if context exists
    if manager.config().get_context(&name).is_none() {
        return Err(anyhow::anyhow!("Context '{}' does not exist", name));
    }

    manager.select_context(name.clone()).await?;
    println!("Switched to context '{}'", name);

    Ok(())
}
