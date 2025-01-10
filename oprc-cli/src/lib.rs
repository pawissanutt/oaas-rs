mod obj;

use http::Uri;
use tracing::info;

#[derive(clap::Parser, Clone, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct OprcCli {
    #[command(subcommand)]
    pub command: OprcCommands,
    #[clap(flatten)]
    connection: ConnectionArgs,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum OprcCommands {
    /// Collection operation
    // #[clap(aliases = &["col", "c"])]
    // Collection {
    //     #[command(subcommand)]
    //     opt: CollectionOperation,
    // },
    /// Object operation
    #[clap(aliases = &["obj", "o"])]
    Object {
        #[command(subcommand)]
        opt: ObjectOperation,
    },
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum CollectionOperation {
    #[clap(aliases = &["c"])]
    Create {
        name: String,
        #[arg(default_value_t = 1)]
        shard_count: u16,
    },
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum ObjectOperation {
    #[clap(aliases = &["s"])]
    Set {
        cls_id: String,
        partition_id: i32,
        id: u64,
        #[arg(short, long)]
        byte_value: Vec<String>,
    },

    #[clap(aliases = &["g"])]
    Get {
        cls_id: String,
        partition_id: u32,
        id: u64,
    },
}

#[derive(clap::Args, Debug, Clone)]
pub struct ConnectionArgs {
    #[arg(short, long, default_value = "http://127.0.0.1:18001")]
    pub server_url: Uri,
    #[arg(short, name = "z", long, default_value = "false")]
    pub zenoh_peer: Option<String>,
}

pub async fn run(cli: OprcCli) {
    info!("use option {cli:?}");
    match cli.command {
        // OprcCommands::Collection { opt } => {}
        OprcCommands::Object { opt } => {
            obj::handle_obj_ops(&opt, &cli.connection).await;
        }
    }
}
