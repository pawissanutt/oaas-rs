mod obj;

use http::Uri;
use tracing::info;

#[derive(clap::Parser, Clone, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct OprcCli {
    #[command(subcommand)]
    pub command: OprcCommands,
    // #[clap(flatten)]
    // conn: ConnectionArgs,
}

#[derive(clap::Subcommand, Clone, Debug)]
pub enum OprcCommands {
    /// Object operation
    #[clap(aliases = &["obj", "o"])]
    Object {
        #[command(subcommand)]
        opt: ObjectOperation,
    },
    /// Invoke operation
    #[clap(aliases = &["ivk", "i"])]
    Invoke {
        #[clap(flatten)]
        opt: InvokeOperation,
        #[clap(flatten)]
        conn: ConnectionArgs,
    },
}

#[derive(clap::Args, Clone, Debug)]
pub struct InvokeOperation {
    /// Class ID
    pub cls_id: String,
    /// Partition ID
    pub partition_id: u16,
    /// Function ID
    pub fn_id: String,
    /// Object ID
    #[arg(short, long)]
    pub object_id: Option<u64>,
    /// Payload as file or stdin if `-` is given. Example: `echo "test" | oprc-cli invoke <cls> <par> <fn> -p -`
    #[arg(short, long)]
    pub payload: Option<clap_stdin::FileOrStdin>,
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
        #[clap(flatten)]
        conn: ConnectionArgs,
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
        #[clap(flatten)]
        conn: ConnectionArgs,
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
    #[arg(long, default_value = "false")]
    pub peer: bool,
}

pub async fn run(cli: OprcCli) {
    info!("use option {cli:?}");
    match &cli.command {
        // OprcCommands::Collection { opt } => {}
        OprcCommands::Object { opt } => {
            obj::handle_obj_ops(&opt).await;
        }
        OprcCommands::Invoke { opt, conn } => {
            obj::handle_invoke_ops(&opt, &conn).await
        }
    }
}
