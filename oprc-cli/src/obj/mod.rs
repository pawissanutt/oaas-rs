use std::{collections::HashMap, process};

use oprc_pb::{
    data_service_client::DataServiceClient, val_data::Data, ObjData, ObjMeta,
    SetObjectRequest, SingleObjectRequest, ValData,
};

use crate::{ConnectionArgs, InvokeOperation, ObjectOperation};
use std::io::{self, Read};

pub async fn handle_obj_ops(opt: &ObjectOperation, connect: &ConnectionArgs) {
    if connect.grpc_url.is_some() {
        handle_obj_ops_grpc(opt, connect).await;
    } else {
        handle_obj_ops_zenoh(opt, connect).await;
    }
}

pub async fn handle_invoke_ops(
    opt: &InvokeOperation,
    connect: &ConnectionArgs,
) {
    let object_proxy = create_proxy(connect).await;

    let mut payload = Vec::new();
    if opt.stdin {
        io::stdin().read_to_end(&mut payload).unwrap_or_else(|e| {
            eprintln!("Failed to read from stdin: {}", e);
            process::exit(1)
        });
    }
    let res = match opt.object_id {
        Some(oid) => {
            let meta = ObjMeta {
                cls_id: opt.cls_id.clone(),
                partition_id: opt.partition_id as u32,
                object_id: oid,
            };
            let res = object_proxy
                .invoke_object_fn(&meta, &opt.fn_id, payload)
                .await;
            res
        }
        None => {
            let res = object_proxy
                .invoke_fn(&opt.cls_id, opt.partition_id, &opt.fn_id, payload)
                .await;
            res
        }
    };

    match res {
        Ok(resp) => {
            if let Some(b) = &resp.payload {
                let str_resp = String::from_utf8_lossy(b);
                print!("{}\n", str_resp);
            } else {
                println!("{:?}", resp);
            }
        }
        Err(err) => {
            eprintln!("Failed to invoke function: {:?}", err);
            process::exit(1);
        }
    }
}

async fn handle_obj_ops_zenoh(opt: &ObjectOperation, connect: &ConnectionArgs) {
    let object_proxy = create_proxy(connect).await;
    match opt {
        ObjectOperation::Set {
            cls_id,
            partition_id,
            id,
            byte_value,
        } => {
            let obj = parse_key_value_pairs(byte_value.clone());
            let obj_data = ObjData {
                entries: obj,
                metadata: Some(ObjMeta {
                    cls_id: cls_id.clone(),
                    partition_id: *partition_id as u32,
                    object_id: *id,
                }),
                ..Default::default()
            };
            let resp = match object_proxy.set_obj(obj_data).await {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("Failed to set object: {:?}", e);
                    process::exit(1);
                }
            };
            print!("Set Successful: {:?}\n", resp);
        }
        ObjectOperation::Get {
            cls_id,
            partition_id,
            id,
        } => {
            let meta = ObjMeta {
                cls_id: cls_id.clone(),
                partition_id: *partition_id as u32,
                object_id: *id,
            };
            let obj = match object_proxy.get_obj(meta).await {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("Failed to get object: {:?}", e);
                    process::exit(1);
                }
            };
            print!("{:?}\n", obj);
        }
    }
}

async fn create_proxy(
    connect: &ConnectionArgs,
) -> oprc_offload::proxy::ObjectProxy {
    let mode = if connect.peer {
        zenoh_config::WhatAmI::Peer
    } else {
        zenoh_config::WhatAmI::Client
    };
    let config = oprc_zenoh::OprcZenohConfig {
        peers: connect.zenoh_peer.clone(),
        zenoh_port: 0,
        mode,
        ..Default::default()
    }
    .create_zenoh();
    let session = match zenoh::open(config).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to open zenoh session: {:?}", e);
            process::exit(1);
        }
    };
    let object_proxy = oprc_offload::proxy::ObjectProxy::new(session);
    object_proxy
}

async fn handle_obj_ops_grpc(opt: &ObjectOperation, connect: &ConnectionArgs) {
    let mut client =
        match DataServiceClient::connect(connect.grpc_url.clone().unwrap())
            .await
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to gRPC server: {:?}", e);
                process::exit(1);
            }
        };
    match opt {
        ObjectOperation::Set {
            cls_id,
            partition_id,
            id,
            byte_value,
        } => {
            let obj = parse_key_value_pairs(byte_value.clone());
            let resp = match client
                .set(SetObjectRequest {
                    cls_id: cls_id.clone(),
                    partition_id: *partition_id as i32,
                    object_id: *id,
                    object: Some(ObjData {
                        entries: obj,
                        ..Default::default()
                    }),
                })
                .await
            {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("Failed to set object: {:?}", e);
                    process::exit(1);
                }
            };
            print!("set success: {:?}\n", resp.into_inner());
        }
        ObjectOperation::Get {
            cls_id,
            partition_id,
            id,
        } => {
            let resp = match client
                .get(SingleObjectRequest {
                    cls_id: cls_id.clone(),
                    partition_id: *partition_id,
                    object_id: *id,
                })
                .await
            {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("Failed to get object: {:?}", e);
                    process::exit(1);
                }
            };
            print!("{:?}\n", resp.into_inner());
        }
    }
}

fn parse_key_value_pairs(pairs: Vec<String>) -> HashMap<u32, ValData> {
    let mut map = HashMap::new();
    for kv in pairs {
        if let Some((key, value)) = kv.split_once('=') {
            match key.parse::<u32>() {
                Ok(parsed_key) => {
                    let b = value.as_bytes().to_vec();
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
