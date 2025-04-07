mod live;
mod obj;

use http::Uri;
use tracing::info;

#[derive(clap::Parser, Clone, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct OprcCli {
    #[command(subcommand)]
    pub command: OprcCommands,
    #[clap(flatten)]
    conn: ConnectionArgs,
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
    },
    /// List liveliness
    #[clap(aliases = &["l"])]
    Liveliness,
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
    /// Print other metadata besides payload
    #[arg(long)]
    pub print_all: bool,
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
    #[arg(short, long, global = true)]
    pub grpc_url: Option<Uri>,
    /// Zenoh peer to connect to if using Zenoh protocol
    #[arg(short = 'z', long, global = true)]
    pub zenoh_peer: Option<String>,
    /// If using zenoh in peer mode
    #[arg(long, default_value = "false", global = true)]
    pub peer: bool,
}

impl ConnectionArgs {
    pub async fn open_zenoh(&self) -> zenoh::Session {
        let mode = if self.peer {
            zenoh_config::WhatAmI::Peer
        } else {
            zenoh_config::WhatAmI::Client
        };
        let config = oprc_zenoh::OprcZenohConfig {
            peers: self.zenoh_peer.clone(),
            zenoh_port: 0,
            gossip_enabled: Some(true),
            mode,
            ..Default::default()
        };
        tracing::debug!("use OprcZenohConfig {:?}", config);
        let config = config.create_zenoh();
        let session = match zenoh::open(config).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to open zenoh session: {:?}", e);
                std::process::exit(1);
            }
        };
        session
    }
}

pub async fn run(cli: OprcCli) {
    let conn = &cli.conn;
    info!("use option {cli:?}");
    match &cli.command {
        // OprcCommands::Collection { opt } => {}
        OprcCommands::Object { opt } => {
            obj::handle_obj_ops(&opt, &conn).await;
        }
        OprcCommands::Invoke { opt } => {
            obj::handle_invoke_ops(&opt, &conn).await
        }
        OprcCommands::Liveliness => live::handle_liveliness(&conn).await,
    }
}
