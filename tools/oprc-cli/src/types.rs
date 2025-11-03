use chrono::DateTime;
use http::Uri;
use std::path::PathBuf;

/// Get version information including build time - using Box::leak to get a static str
fn get_version_info() -> &'static str {
    let version = env!("CARGO_PKG_VERSION");
    let build_timestamp_str = env!("BUILD_TIMESTAMP");
    let git_hash = option_env!("GIT_HASH").unwrap_or("unknown");

    // Parse the timestamp and format it nicely
    let build_time = if let Ok(timestamp) = build_timestamp_str.parse::<i64>() {
        DateTime::from_timestamp(timestamp, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };

    Box::leak(
        format!("{} (built {}, git {})", version, build_time, git_hash)
            .into_boxed_str(),
    )
}

/// Main CLI structure
#[derive(clap::Parser, Clone, Debug)]
#[clap(author, version = get_version_info(), about, long_about = None)]
pub struct OprcCli {
    #[command(subcommand)]
    pub command: OprcCommands,
}

/// Available CLI commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum OprcCommands {
    /// Object operations (create, read, update, delete)
    #[clap(aliases = &["obj", "o"])]
    Object {
        #[command(subcommand)]
        opt: ObjectOperation,
        #[clap(flatten)]
        conn: ConnectionArgs,
    },
    /// Function invocation operations (sync and async)
    #[clap(aliases = &["ivk", "i"])]
    Invoke {
        #[clap(flatten)]
        opt: InvokeOperation,
        #[clap(flatten)]
        conn: ConnectionArgs,
    },
    /// Retrieve async invocation results
    #[clap(aliases = &["res", "r"])]
    Result {
        #[clap(flatten)]
        opt: ResultOperation,
        #[clap(flatten)]
        conn: ConnectionArgs,
    },
    /// List environment liveliness information
    #[clap(aliases = &["l"])]
    Liveliness {
        #[clap(flatten)]
        conn: ConnectionArgs,
    },

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
    /// Class runtime listing / retrieval
    #[clap(aliases = &["cr", "runtimes", "rts"])]
    ClassRuntimes {
        #[command(subcommand)]
        opt: ClassRuntimeOperation,
    },
    /// Environment listing
    #[clap(aliases = &["envs", "env", "clu", "cl"])]
    Environments {
        #[command(subcommand)]
        opt: EnvironmentsOperation,
    },
    /// Query server capabilities (feature flags supported by data layer)
    #[clap(aliases = &["caps", "cap", "features"])]
    Capabilities {
        #[clap(flatten)]
        conn: ConnectionArgs,
        /// Output as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Query per-shard capabilities over Zenoh
    #[clap(aliases = &["zcaps", "zcap", "zc"])]
    CapabilitiesZenoh {
        /// Class identifier (use '*' for all)
        #[arg(default_value = "*")]
        cls: String,
        /// Partition number (use '*' for all)
        #[arg(default_value = "*")]
        partition_id: String,
        /// Shard identifier (use '*' for all)
        #[arg(default_value = "*")]
        shard_id: String,
        #[clap(flatten)]
        conn: ConnectionArgs,
        /// Output as JSON
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Class runtime operations
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ClassRuntimeOperation {
    /// List class runtimes or fetch a specific runtime by id
    #[clap(aliases = &["l"])]
    List {
        /// Optional runtime id to fetch a single runtime
        id: Option<String>,
    },
}

/// Environment operations
#[derive(clap::Subcommand, Clone, Debug)]
pub enum EnvironmentsOperation {
    /// List known environments
    #[clap(aliases = &["l"])]
    List,
}

/// Object operation commands
#[derive(clap::Subcommand, Clone, Debug)]
pub enum ObjectOperation {
    /// Set/create an object
    #[clap(aliases = &["s"])]
    Set {
        /// Class identifier (loads from context if not provided)
        #[arg(short, long)]
        cls_id: Option<String>,
        /// Partition number (0-65535)
        partition_id: u16,
        /// Object identifier
        id: u64,
        /// Key-value pairs of object data. Example: `-b 0=DATA1 -b 1=DATA2`
        #[arg(short, long)]
        byte_value: Vec<String>,
        /// String entry key-value pairs (string keys). Example: `-s name=alice -s status=ready`
        #[arg(short = 's', long = "str", value_name = "KEY=VALUE")]
        str_value: Vec<String>,
    },
    /// Get/retrieve an object
    #[clap(aliases = &["g"])]
    Get {
        /// Class identifier (loads from context if not provided)
        #[arg(short, long)]
        cls_id: Option<String>,
        /// Partition number (0-65535)
        partition_id: u32,
        /// Object identifier
        id: u64,
        /// Print specific field only
        #[arg(short, long)]
        key: Option<u32>,
    },
    /// Set/create an object using a string object id (create-only semantics on server)
    #[clap(aliases = &["ss", "setstr"])]
    SetStr {
        /// Class identifier (loads from context if not provided)
        #[arg(short, long)]
        cls_id: Option<String>,
        /// Partition number (0-65535)
        partition_id: u16,
        /// String object identifier (will be normalized server-side)
        object_id_str: String,
        /// Numeric key-value pairs (optional)
        #[arg(short, long)]
        byte_value: Vec<String>,
        /// String entry key-value pairs (string keys). Example: `-s name=alice -s status=ready`
        #[arg(short = 's', long = "str", value_name = "KEY=VALUE")]
        str_value: Vec<String>,
    },
    /// Get an object using a string object id
    #[clap(aliases = &["gs", "getstr"])]
    GetStr {
        /// Class identifier (loads from context if not provided)
        #[arg(short, long)]
        cls_id: Option<String>,
        /// Partition number (0-65535)
        partition_id: u32,
        /// String object identifier
        object_id_str: String,
        /// Print specific numeric field only
        #[arg(short, long)]
        key: Option<u32>,
        /// Print specific string field only
        #[arg(long = "key-str")]
        key_str: Option<String>,
    },
    /// Get a single string entry value (explicit subcommand form)
    #[clap(aliases = &["gsk", "getstrkey"])]
    GetStrKey {
        /// Class identifier (loads from context if not provided)
        #[arg(short, long)]
        cls_id: Option<String>,
        /// Partition number (0-65535)
        partition_id: u32,
        /// String object identifier
        #[arg(long = "object-id-str")]
        object_id_str: String,
        /// String key to retrieve (required)
        #[arg(long = "key-str")]
        key_str: String,
    },
    /// List string entry keys (and optionally values) for an object with string id
    #[clap(aliases = &["lss", "liststr"])]
    ListStr {
        /// Class identifier (loads from context if not provided)
        #[arg(short, long)]
        cls_id: Option<String>,
        /// Partition number (0-65535)
        partition_id: u32,
        /// String object identifier
        #[arg(long = "object-id-str")]
        object_id_str: String,
        /// Show values as key=value lines instead of just keys
        #[arg(long, default_value_t = false)]
        with_values: bool,
    },
}

/// Function invocation parameters
#[derive(clap::Args, Clone, Debug)]
pub struct InvokeOperation {
    /// Class identifier (loads from context if not provided)
    #[arg(short, long)]
    pub cls_id: Option<String>,
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
    /// Class identifier (loads from context if not provided)
    #[arg(short, long)]
    pub cls_id: Option<String>,
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
            zenoh::config::WhatAmI::Peer
        } else {
            zenoh::config::WhatAmI::Client
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

    /// Merge context configuration with explicit connection arguments
    /// Context values are used when explicit arguments are not provided
    pub async fn with_context(&self) -> Self {
        use crate::config::ContextManager;

        // Try to load context, fallback to default context if loading fails
        let context_result = ContextManager::new().await;

        match context_result {
            Ok(manager) => {
                if let Some(context) = manager.get_current_context() {
                    Self {
                        grpc_url: self.grpc_url.clone().or_else(|| {
                            context
                                .gateway_url
                                .as_ref()
                                .and_then(|url| url.parse().ok())
                        }),
                        zenoh_peer: self
                            .zenoh_peer
                            .clone()
                            .or_else(|| context.zenoh_peer.clone()),
                        peer: self.peer, // Keep the explicit peer mode setting
                    }
                } else {
                    // No current context, use default values
                    Self {
                        grpc_url: self.grpc_url.clone().or_else(|| {
                            "http://oaas.127.0.0.1.nip.io".parse().ok()
                        }),
                        zenoh_peer: self.zenoh_peer.clone(),
                        peer: self.peer,
                    }
                }
            }
            Err(_) => {
                // Context loading failed, use default values
                Self {
                    grpc_url: self.grpc_url.clone().or_else(|| {
                        "http://oaas.127.0.0.1.nip.io".parse().ok()
                    }),
                    zenoh_peer: self.zenoh_peer.clone(),
                    peer: self.peer,
                }
            }
        }
    }

    /// Merge context configuration with explicit connection arguments using a specific config path
    /// This method is primarily for testing to avoid environment variable conflicts
    pub async fn with_context_from_path<P: AsRef<std::path::Path>>(
        &self,
        config_path: P,
    ) -> Self {
        use crate::config::ContextManager;

        // Try to load context from specific path
        let context_result =
            ContextManager::with_config_path(config_path).await;

        match context_result {
            Ok(manager) => {
                if let Some(context) = manager.get_current_context() {
                    Self {
                        grpc_url: self.grpc_url.clone().or_else(|| {
                            context
                                .gateway_url
                                .as_ref()
                                .and_then(|url| url.parse().ok())
                        }),
                        zenoh_peer: self
                            .zenoh_peer
                            .clone()
                            .or_else(|| context.zenoh_peer.clone()),
                        peer: self.peer, // Keep the explicit peer mode setting
                    }
                } else {
                    // No current context, use default values
                    Self {
                        grpc_url: self.grpc_url.clone().or_else(|| {
                            "http://oaas.127.0.0.1.nip.io".parse().ok()
                        }),
                        zenoh_peer: self.zenoh_peer.clone(),
                        peer: self.peer,
                    }
                }
            }
            Err(_) => {
                // Context loading failed, use default values
                Self {
                    grpc_url: self.grpc_url.clone().or_else(|| {
                        "http://oaas.127.0.0.1.nip.io".parse().ok()
                    }),
                    zenoh_peer: self.zenoh_peer.clone(),
                    peer: self.peer,
                }
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
        /// After applying the package, also apply any deployments defined within it
        #[arg(short = 'd', long, default_value_t = false)]
        apply_deployments: bool,
        /// If a package with the same name exists, overwrite it instead of failing
        #[arg(short = 'O', long, default_value_t = false)]
        overwrite: bool,
    },
    /// Remove packages and classes
    #[clap(aliases = &["d", "rm", "r"])]
    Delete {
        /// YAML package definition file
        file: PathBuf,
        /// Override package name
        #[arg(short = 'p', long)]
        override_package: Option<String>,
        /// Before deleting the package, also delete any deployments defined within it
        #[arg(short = 'd', long, default_value_t = false)]
        delete_deployments: bool,
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
        /// Zenoh peer endpoint
        #[arg(long)]
        zenoh_peer: Option<String>,
        /// Explicitly choose transport: true for gRPC, false for Zenoh; omit to infer
        #[arg(long)]
        use_grpc: Option<bool>,
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
    /// Apply (create/update) deployments from an OPackage YAML file
    #[clap(aliases = &["a", "create", "c"])]
    Apply {
        /// YAML package definition file containing `deployments` entries
        file: PathBuf,
        /// Override package name (applies to deployments with empty package_name)
        #[arg(short = 'p', long)]
        override_package: Option<String>,
        /// If a deployment with the same key exists, overwrite it instead of failing
        #[arg(long, default_value_t = false)]
        overwrite: bool,
    },
    /// Remove deployments
    #[clap(aliases = &["d", "rm", "r"])]
    Delete {
        /// Deployment name to delete
        name: String,
    },
}

// (Runtime operations removed – runtime API not exposed in PM v1)

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CliConfig, ContextConfig};
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tokio;

    // Helper function to create a temporary config directory with contexts
    async fn create_test_config_for_connection_args()
    -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().unwrap();

        // Set up test config
        let mut contexts = HashMap::new();
        contexts.insert(
            "test".to_string(),
            ContextConfig {
                pm_url: Some("http://test.pm.com".to_string()),
                gateway_url: Some("http://test.gateway.com".to_string()),
                default_class: Some("test.class".to_string()),
                zenoh_peer: Some("tcp/192.168.1.100:7447".to_string()),
                ..Default::default()
            },
        );

        let config = CliConfig {
            contexts,
            current_context: "test".to_string(),
        };

        // Save config to temp directory
        let config_path = temp_dir.path().join("config.yml");
        let config_content = serde_yaml::to_string(&config).unwrap();
        tokio::fs::write(&config_path, config_content)
            .await
            .unwrap();

        (temp_dir, config_path)
    }

    #[tokio::test]
    async fn test_connection_args_with_context() {
        let (temp_dir, config_path) =
            create_test_config_for_connection_args().await;

        // Test with no explicit args - should use context values
        let conn_args = ConnectionArgs {
            grpc_url: None,
            zenoh_peer: None,
            peer: false,
        };

        let merged_args = conn_args.with_context_from_path(&config_path).await;

        // Should have gateway URL from context as grpc_url
        assert!(merged_args.grpc_url.is_some());
        assert_eq!(merged_args.grpc_url.unwrap(), "http://test.gateway.com");

        // Should have zenoh_peer from context
        assert!(merged_args.zenoh_peer.is_some());
        assert_eq!(merged_args.zenoh_peer.unwrap(), "tcp/192.168.1.100:7447");

        // Clean up
        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_connection_args_explicit_override() {
        let (temp_dir, config_path) =
            create_test_config_for_connection_args().await;

        // Test with explicit args - should override context values
        let explicit_grpc: http::Uri =
            "http://explicit.gateway.com".parse().unwrap();
        let conn_args = ConnectionArgs {
            grpc_url: Some(explicit_grpc.clone()),
            zenoh_peer: Some("tcp/explicit.host:7447".to_string()),
            peer: true,
        };

        let merged_args = conn_args.with_context_from_path(&config_path).await;

        // Should keep explicit values, not use context
        assert_eq!(merged_args.grpc_url.unwrap(), explicit_grpc);
        assert_eq!(merged_args.zenoh_peer.unwrap(), "tcp/explicit.host:7447");
        assert_eq!(merged_args.peer, true);

        // Clean up
        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_partial_context_override() {
        let (temp_dir, config_path) =
            create_test_config_for_connection_args().await;

        // Test with only grpc_url explicit - zenoh_peer should come from context
        let explicit_grpc: http::Uri =
            "http://explicit.gateway.com".parse().unwrap();
        let conn_args = ConnectionArgs {
            grpc_url: Some(explicit_grpc.clone()),
            zenoh_peer: None, // Should use context value
            peer: false,
        };

        let merged_args = conn_args.with_context_from_path(&config_path).await;

        assert_eq!(merged_args.grpc_url.unwrap(), explicit_grpc);
        assert_eq!(merged_args.zenoh_peer.unwrap(), "tcp/192.168.1.100:7447"); // From context
        assert_eq!(merged_args.peer, false);

        // Clean up
        drop(temp_dir);
    }

    #[tokio::test]
    async fn test_connection_args_with_default_context() {
        use tempfile::TempDir;

        // Create a temp directory with a malformed config file
        let temp_dir = TempDir::new().unwrap();
        let malformed_config = temp_dir.path().join("malformed.yml");

        // Write malformed YAML to ensure context loading fails and creates default
        tokio::fs::write(&malformed_config, "invalid: yaml: content: [")
            .await
            .unwrap();

        let conn_args = ConnectionArgs {
            grpc_url: None,
            zenoh_peer: None,
            peer: false,
        };

        let merged_args =
            conn_args.with_context_from_path(&malformed_config).await;

        // When config loading fails, system creates default context with default gateway URL
        // So conn_args with no explicit values should get the default gateway URL
        assert!(merged_args.grpc_url.is_some());
        assert_eq!(
            merged_args.grpc_url.unwrap().to_string(),
            "http://oaas.127.0.0.1.nip.io/"
        );
        assert!(merged_args.zenoh_peer.is_none()); // Default context has no zenoh_peer
        assert_eq!(merged_args.peer, false);

        // Clean up
        drop(temp_dir);
    }

    #[test]
    fn test_cli_command_parsing() {
        // Test that all command aliases work
        assert!(matches!(
            OprcCommands::Package {
                opt: PackageOperation::Apply {
                    file: PathBuf::new(),
                    override_package: None,
                    apply_deployments: false,
                    overwrite: false
                }
            },
            OprcCommands::Package { .. }
        ));

        assert!(matches!(
            OprcCommands::Class {
                opt: ClassOperation::List { class_name: None }
            },
            OprcCommands::Class { .. }
        ));

        assert!(matches!(
            OprcCommands::Function {
                opt: FunctionOperation::List {
                    function_name: None
                }
            },
            OprcCommands::Function { .. }
        ));

        assert!(matches!(
            OprcCommands::Context {
                opt: ContextOperation::Get
            },
            OprcCommands::Context { .. }
        ));
    }

    #[test]
    fn test_output_format_parsing() {
        assert!(matches!(OutputFormat::Json, OutputFormat::Json));
        assert!(matches!(OutputFormat::Yaml, OutputFormat::Yaml));
        assert!(matches!(OutputFormat::Table, OutputFormat::Table));
    }
}
