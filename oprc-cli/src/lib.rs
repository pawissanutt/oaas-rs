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
    /// Invoke operation
    #[clap(aliases = &["obj", "o"])]
    Invoke {
        #[clap(flatten)]
        opt: InvokeOperation,
    },
}

#[derive(clap::Args, Clone, Debug)]
pub struct InvokeOperation {
    pub cls_id: String,
    pub fn_id: String,
    #[arg(short, long)]
    pub partition_id: Option<u16>,
    #[arg(short, long)]
    pub object_id: Option<u64>,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum CollectionOperation {
    #[clap(aliases = &["c"])]
    Create {
        /// Collection name
        name: String,
        /// Number of partitions
        #[arg(default_value_t = 1)]
        partition_count: u16,
    },
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum ObjectOperation {
    /// Set object
    #[clap(aliases = &["s"])]
    Set {
        /// Class ID
        cls_id: String,
        /// Partition ID
        partition_id: u16,
        /// Object ID
        id: u64,
        /// Key-value pairs of object data. Example `-b 0=THIS_IS_DATA -b 1=ANOTHER_DATA`
        #[arg(short, long)]
        byte_value: Vec<String>,
    },

    /// Get object
    #[clap(aliases = &["g"])]
    Get {
        /// Class ID
        cls_id: String,
        /// Partition ID
        partition_id: u32,
        /// Object ID
        id: u64,
    },
    // #[clap(aliases = &["d"])]
    // Delete {
    //     cls_id: String,
    //     partition_id: u32,
    //     id: u64,
    // },
}

#[derive(clap::Args, Debug, Clone)]
pub struct ConnectionArgs {
    /// Server URL if using gRPC protocol
    #[arg(short, long)]
    pub grpc_url: Option<Uri>,
    /// Zenoh peer to connect to if using Zenoh protocol
    #[arg(short, name = "z", long)]
    pub zenoh_peer: Option<String>,
    /// If using zenoh in peer mode
    #[arg(short, long, default_value = "false")]
    pub peer_mode: bool,
}

pub async fn run(cli: OprcCli) {
    info!("use option {cli:?}");
    match cli.command {
        // OprcCommands::Collection { opt } => {}
        OprcCommands::Object { opt } => {
            obj::handle_obj_ops(&opt, &cli.connection).await;
        }
        OprcCommands::Invoke { opt } => {
            obj::handle_invoke_ops(&opt, &cli.connection).await
        }
    }
}
