use http::Uri;
use std::path::PathBuf;

/// Main CLI structure
#[derive(clap::Parser, Clone, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct OprcCli {
    #[command(subcommand)]
    pub command: OprcCommands,
    #[clap(flatten)]
    pub conn: ConnectionArgs,
}

/// Available CLI commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum OprcCommands {
    /// Object operations (create, read, update, delete)
    #[clap(aliases = &["obj", "o"])]
    Object {
        #[command(subcommand)]
        opt: ObjectOperation,
    },
    /// Function invocation operations (sync and async)
    #[clap(aliases = &["ivk", "i"])]
    Invoke {
        #[clap(flatten)]
        opt: InvokeOperation,
    },
    /// Retrieve async invocation results
    #[clap(aliases = &["res", "r"])]
    Result {
        #[clap(flatten)]
        opt: ResultOperation,
    },
    /// List cluster liveliness information
    #[clap(aliases = &["l"])]
    Liveliness,
    
    // New OCLI commands
    /// Package management operations
    #[clap(aliases = &["pkg", "p"])]
    Package {
        #[command(subcommand)]
        opt: PackageOperation,
    },
    /// Class management operations
    #[clap(aliases = &["cls", "c"])]
    Class {
        #[command(subcommand)]
        opt: ClassOperation,
    },
    /// Function management operations
    #[clap(aliases = &["fn", "f"])]
    Function {
        #[command(subcommand)]
        opt: FunctionOperation,
    },
    /// Context management operations
    #[clap(aliases = &["ctx"])]
    Context {
        #[command(subcommand)]
        opt: ContextOperation,
    },
    /// Deployment management operations
    #[clap(aliases = &["dep", "de"])]
    Deploy {
        #[command(subcommand)]
        opt: DeployOperation,
    },
    /// Class runtime management operations
    #[clap(aliases = &["cr"])]
    ClassRuntime {
        #[command(subcommand)]
        opt: RuntimeOperation,
    },
}

/// Object operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ObjectOperation {
    /// Set/create an object
    #[clap(aliases = &["s"])]
    Set {
        /// Class identifier
        cls_id: String,
        /// Partition number (0-65535)
        partition_id: u16,
        /// Object identifier
        id: u64,
        /// Key-value pairs of object data. Example: `-b 0=DATA1 -b 1=DATA2`
        #[arg(short, long)]
        byte_value: Vec<String>,
    },
    /// Get/retrieve an object
    #[clap(aliases = &["g"])]
    Get {
        /// Class identifier
        cls_id: String,
        /// Partition number (0-65535)
        partition_id: u32,
        /// Object identifier
        id: u64,
        /// Print specific field only
        #[arg(short, long)]
        key: Option<u32>,
    },
}

/// Function invocation parameters
#[derive(clap::Args, Clone, Debug)]
pub struct InvokeOperation {
    /// Class identifier
    pub cls_id: String,
    /// Partition number (0-65535)
    pub partition_id: u16,
    /// Function/method identifier
    pub fn_id: String,
    /// Object ID for stateful method calls
    #[arg(short, long)]
    pub object_id: Option<u64>,
    /// Function payload as file path or stdin (use `-` for stdin)
    /// Example: `echo "data" | oprc-cli invoke <cls> <par> <fn> -p -`
    #[arg(short, long)]
    pub payload: Option<clap_stdin::FileOrStdin>,
    /// Print additional metadata (status, headers, etc.)
    #[arg(long)]
    pub print_all: bool,
    /// Execute asynchronously (fire-and-forget pattern)
    #[arg(long)]
    pub async_mode: bool,
    /// Custom invocation ID for async operations (auto-generated if not provided)
    #[arg(long)]
    pub invocation_id: Option<String>,
}

/// Async result retrieval parameters
#[derive(clap::Args, Clone, Debug)]
pub struct ResultOperation {
    /// Class identifier
    pub cls_id: String,
    /// Partition number (0-65535)
    pub partition_id: u16,
    /// Function/method identifier
    pub fn_id: String,
    /// Invocation ID from async operation
    pub invocation_id: String,
    /// Object ID for stateful method results
    #[arg(short, long)]
    pub object_id: Option<u64>,
    /// Continuously monitor for result (not yet implemented)
    #[arg(short, long)]
    pub watch: bool,
    /// Print additional metadata (status, headers, etc.)
    #[arg(long)]
    pub print_all: bool,
}

/// Connection configuration
#[derive(clap::Args, Debug, Clone)]
pub struct ConnectionArgs {
    /// gRPC server URL for direct connection
    #[arg(short, long, global = true)]
    pub grpc_url: Option<Uri>,
    /// Zenoh peer endpoint for distributed connection
    #[arg(short = 'z', long, global = true)]
    pub zenoh_peer: Option<String>,
    /// Use Zenoh in peer mode (vs client mode)
    #[arg(long, default_value = "false", global = true)]
    pub peer: bool,
}

impl ConnectionArgs {
    /// Create a Zenoh session based on the connection configuration
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
        
        tracing::debug!("using Zenoh config: {:?}", config);
        let zenoh_config = config.create_zenoh();
        
        match zenoh::open(zenoh_config).await {
            Ok(session) => session,
            Err(e) => {
                eprintln!("Failed to open Zenoh session: {:?}", e);
                std::process::exit(1);
            }
        }
    }
}

/// Package operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum PackageOperation {
    /// Deploy/update packages from YAML
    #[clap(aliases = &["a", "create", "c"])]
    Apply {
        /// YAML package definition file
        file: PathBuf,
        /// Override package name
        #[arg(short = 'p', long)]
        override_package: Option<String>,
    },
    /// Remove packages and classes
    #[clap(aliases = &["d", "rm", "r"])]
    Delete {
        /// YAML package definition file
        file: PathBuf,
        /// Override package name
        #[arg(short = 'p', long)]
        override_package: Option<String>,
    },
}

/// Class operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ClassOperation {
    /// List available classes
    #[clap(aliases = &["l"])]
    List {
        /// Optional class name filter
        class_name: Option<String>,
    },
    /// Delete specific classes
    Delete {
        /// Class name to delete
        class_name: String,
    },
}

/// Function operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum FunctionOperation {
    /// List available functions
    #[clap(aliases = &["l"])]
    List {
        /// Optional function name filter
        function_name: Option<String>,
    },
}

/// Context operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ContextOperation {
    /// Configure connection settings
    #[clap(aliases = &["s", "update"])]
    Set {
        /// Context name (defaults to current)
        name: Option<String>,
        /// Package manager URL
        #[arg(long)]
        pm: Option<String>,
        /// Gateway URL
        #[arg(long)]
        gateway: Option<String>,
        /// Default class name
        #[arg(long)]
        cls: Option<String>,
    },
    /// Display current configuration
    #[clap(aliases = &["g"])]
    Get,
    /// Switch between contexts
    Select {
        /// Context name to switch to
        name: String,
    },
}

/// Deployment operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum DeployOperation {
    /// List current deployments
    #[clap(aliases = &["l"])]
    List {
        /// Optional deployment filter
        name: Option<String>,
    },
    /// Remove deployments
    Delete {
        /// Deployment name to delete
        name: String,
    },
}

/// Runtime operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum RuntimeOperation {
    /// List active class runtimes
    #[clap(aliases = &["l"])]
    List {
        /// Optional runtime filter
        name: Option<String>,
    },
    /// Remove runtime instances
    Delete {
        /// Runtime name to delete
        name: String,
    },
}

/// Output formatting options
#[derive(clap::Args, Clone, Debug)]
pub struct OutputArgs {
    /// Output format
    #[arg(short = 'o', long, value_enum, default_value = "json")]
    pub output: OutputFormat,
}

/// Available output formats
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Json,
    Yaml,
    Table,
}
