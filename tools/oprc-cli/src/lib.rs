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
    // Merge context configuration with explicit connection arguments
    let conn = cli.conn.with_context().await;

    match &cli.command {
        OprcCommands::Object { opt } => {
            let use_grpc = should_use_grpc(&conn).await;
            obj::handle_obj_ops(opt, &conn, use_grpc).await;
        }
        OprcCommands::Invoke { opt } => {
            let use_grpc = should_use_grpc(&conn).await;
            obj::handle_invoke_ops(opt, &conn, use_grpc).await;
        }
        OprcCommands::Result { opt } => {
            let use_grpc = should_use_grpc(&conn).await;
            obj::handle_result_ops(opt, &conn, use_grpc).await;
        }
        OprcCommands::Liveliness => {
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
        OprcCommands::ClassRuntimes { id } => {
            if let Err(e) = commands::handle_class_runtimes_command(id).await {
                eprintln!("Class runtimes command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::DeploymentStatus { id } => {
            if let Err(e) = commands::handle_deployment_status_command(id).await
            {
                eprintln!("Deployment status command failed: {}", e);
                process::exit(1);
            }
        }
        OprcCommands::Environments => {
            if let Err(e) = commands::handle_envs_command().await {
                eprintln!("Clusters command failed: {}", e);
                process::exit(1);
            }
        }
    }
}

async fn should_use_grpc(conn: &ConnectionArgs) -> bool {
    // Decide transport based on context config (prefer Zenoh when configured)
    match config::ContextManager::new().await {
        Ok(manager) => manager
            .get_current_context()
            .map(|ctx| {
                if let Some(flag) = ctx.use_grpc {
                    flag
                } else {
                    // Infer: if zenoh configured, don't use gRPC; else use gRPC
                    ctx.zenoh_peer.is_none()
                }
            })
            .unwrap_or_else(|| conn.grpc_url.is_some()),
        Err(_) => conn.grpc_url.is_some(),
    }
}
