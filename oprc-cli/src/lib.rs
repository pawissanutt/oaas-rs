use std::{collections::HashMap, error::Error};

use bytes::Bytes;
use http::Uri;
use oprc_pb::{
    data_service_client::DataServiceClient, val_data::Data, ObjData,
    SetObjectRequest, SingleObjectRequest, ValData,
};
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
}

pub async fn run(cli: OprcCli) -> Result<(), Box<dyn Error>> {
    info!("use option {cli:?}");
    match cli.command {
        // OprcCommands::Collection { opt } => {}
        OprcCommands::Object { opt } => {
            handle_obj_ops(&opt, &cli.connection).await?;
        }
    }
    Ok(())
}

fn parse_key_value_pairs(pairs: Vec<String>) -> HashMap<u32, ValData> {
    let mut map = HashMap::new();
    for kv in pairs {
        if let Some((key, value)) = kv.split_once('=') {
            match key.parse::<u32>() {
                Ok(parsed_key) => {
                    let b = Bytes::from(value.to_string());
                    let val = ValData {
                        data: Some(Data::Byte(b)),
                    };
                    map.insert(parsed_key, val);
                }
                Err(e) => {
                    eprintln!("Failed to parse key '{}': {}", key, e);
                }
            }
        } else {
            eprintln!("Invalid key-value format: {}", kv);
        }
    }
    map
}

async fn handle_obj_ops(
    opt: &ObjectOperation,
    connect: &ConnectionArgs,
) -> Result<(), Box<dyn Error>> {
    let mut client =
        DataServiceClient::connect(connect.server_url.clone()).await?;
    match opt {
        ObjectOperation::Set {
            cls_id,
            partition_id,
            id,
            byte_value,
        } => {
            let obj = parse_key_value_pairs(byte_value.clone());
            let resp = client
                .set(SetObjectRequest {
                    cls_id: cls_id.clone(),
                    partition_id: *partition_id,
                    object_id: *id,
                    object: Some(ObjData {
                        entries: obj,
                        ..Default::default()
                    }),
                })
                .await?;
            print!("set success: {:?}\n", resp.into_inner());
        }
        ObjectOperation::Get {
            cls_id,
            partition_id,
            id,
        } => {
            let resp = client
                .get(SingleObjectRequest {
                    cls_id: cls_id.clone(),
                    partition_id: *partition_id,
                    object_id: *id,
                })
                .await?;
            print!("{:?}\n", resp.into_inner());
        }
    }

    Ok(())
}
