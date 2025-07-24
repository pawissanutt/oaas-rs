mod live;
mod obj;
mod types;
mod config;
mod client;

use tracing::info;

pub use types::{
    ConnectionArgs, InvokeOperation, ObjectOperation, OprcCli, OprcCommands,
    ResultOperation, PackageOperation, ClassOperation, FunctionOperation,
    ContextOperation, DeployOperation, RuntimeOperation, OutputFormat,
};

pub async fn run(cli: OprcCli) {
    let conn = &cli.conn;
    info!("use option {cli:?}");

    match &cli.command {
        OprcCommands::Object { opt } => {
            obj::handle_obj_ops(opt, conn).await;
        }
        OprcCommands::Invoke { opt } => {
            obj::handle_invoke_ops(opt, conn).await;
        }
        OprcCommands::Result { opt } => {
            obj::handle_result_ops(opt, conn).await;
        }
        OprcCommands::Liveliness => {
            live::handle_liveliness(conn).await;
        }
        // New OCLI commands - placeholder implementations
        OprcCommands::Package { opt } => {
            println!("Package command not yet implemented: {:?}", opt);
        }
        OprcCommands::Class { opt } => {
            println!("Class command not yet implemented: {:?}", opt);
        }
        OprcCommands::Function { opt } => {
            println!("Function command not yet implemented: {:?}", opt);
        }
        OprcCommands::Context { opt } => {
            println!("Context command not yet implemented: {:?}", opt);
        }
        OprcCommands::Deploy { opt } => {
            println!("Deploy command not yet implemented: {:?}", opt);
        }
        OprcCommands::ClassRuntime { opt } => {
            println!("ClassRuntime command not yet implemented: {:?}", opt);
        }
    }
}
