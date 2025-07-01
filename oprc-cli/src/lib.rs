mod live;
mod obj;
mod types;

use tracing::info;

pub use types::{
    ConnectionArgs, InvokeOperation, ObjectOperation, OprcCli, OprcCommands,
    ResultOperation,
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
    }
}
