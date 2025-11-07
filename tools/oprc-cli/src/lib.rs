mod client;
mod commands;
mod config;
mod live;
mod obj;
mod output;
mod types;

use std::process;
// use tracing::info;

pub use output::{OutputArgs, print_output};
pub use types::{
    ClassOperation, ConnectionArgs, ContextOperation, DeployOperation,
    FunctionOperation, InvokeOperation, ObjectOperation, OprcCli, OprcCommands,
    OutputFormat, PackageOperation, ResultOperation,
};

pub async fn run(cli: OprcCli) {
    match &cli.command {
        OprcCommands::Object { opt, conn } => {
            let use_grpc = should_use_grpc(conn).await;
            let conn = conn.with_context().await;
            obj::handle_obj_ops(opt, &conn, use_grpc).await;
        }
        OprcCommands::Invoke { opt, conn } => {
            let use_grpc = should_use_grpc(conn).await;
            let conn = conn.with_context().await;
            obj::handle_invoke_ops(opt, &conn, use_grpc).await;
        }
        OprcCommands::Result { opt, conn } => {
            let use_grpc = should_use_grpc(conn).await;
            let conn = conn.with_context().await;
            obj::handle_result_ops(opt, &conn, use_grpc).await;
        }
        OprcCommands::Liveliness { conn } => {
            let conn = conn.with_context().await;
            live::handle_liveliness(&conn).await;
        }
        // New OCLI commands with proper implementations
        OprcCommands::Package { opt } => {
            if let Err(e) = commands::handle_package_command(opt).await {
                eprintln!("Package command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Class { opt } => {
            if let Err(e) = commands::handle_class_command(opt).await {
                eprintln!("Class command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Function { opt } => {
            if let Err(e) = commands::handle_function_command(opt).await {
                eprintln!("Function command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Context { opt } => {
            if let Err(e) = commands::handle_context_command(opt).await {
                eprintln!("Context command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Deploy { opt } => {
            if let Err(e) = commands::handle_deploy_command(opt).await {
                eprintln!("Deploy command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::ClassRuntimes { opt } => {
            if let Err(e) = match opt {
                types::ClassRuntimeOperation::List { id } => {
                    commands::handle_class_runtimes_command(id).await
                }
            } {
                eprintln!("Class runtimes command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Environments { opt } => {
            if let Err(e) = match opt {
                types::EnvironmentsOperation::List => {
                    commands::handle_envs_command().await
                }
            } {
                eprintln!("Clusters command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Capabilities { conn, json } => {
            let conn = conn.with_context().await;
            if let Err(e) =
                commands::handle_capabilities_command(&conn, *json).await
            {
                eprintln!("Capabilities command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::CapabilitiesZenoh {
            cls,
            partition_id,
            shard_id,
            conn,
            json,
        } => {
            let conn = conn.with_context().await;
            if let Err(e) = commands::handle_capabilities_zenoh_command(
                &conn,
                cls,
                partition_id,
                shard_id,
                *json,
            )
            .await
            {
                eprintln!("Capabilities (zenoh) command failed: {}", e);
                process::exit(1);
            }
        }
    }
}

async fn should_use_grpc(raw_conn_args: &ConnectionArgs) -> bool {
    // 1) Prioritize explicit connection args
    if raw_conn_args.grpc_url.is_some() {
        return true;
    }
    if raw_conn_args.zenoh_peer.is_some() {
        return false;
    }

    // 2) Use context preference: when Some(true) => gRPC (using gateway_url), else Zenoh
    match config::ContextManager::new().await {
        Ok(manager) => manager
            .get_current_context()
            .and_then(|ctx| ctx.use_grpc)
            .unwrap_or(false),
        Err(_) => false,
    }
}
