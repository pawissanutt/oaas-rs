use std::process;

use oprc_pb::{ObjData, ObjMeta};

use crate::{ConnectionArgs, InvokeOperation, ObjectOperation};

use super::util::extract_payload;

pub async fn invoke_func(
    opt: &InvokeOperation,
    connect: &ConnectionArgs,
) -> anyhow::Result<oprc_pb::InvocationResponse> {
    let payload = extract_payload(opt);
    let object_proxy = create_proxy(connect).await;
    match opt.object_id {
        Some(oid) => {
            let meta = ObjMeta {
                cls_id: opt.cls_id.clone(),
                partition_id: opt.partition_id as u32,
                object_id: oid,
            };
            let res = object_proxy
                .invoke_object_fn(&meta, &opt.fn_id, payload)
                .await?;
            // object_proxy
            //     .close()
            //     .await
            //     .expect("Failed to close object proxy");
            anyhow::Ok(res)
        }
        None => {
            let res = object_proxy
                .invoke_fn(&opt.cls_id, opt.partition_id, &opt.fn_id, payload)
                .await?;
            // object_proxy
            //     .close()
            //     .await
            //     .expect("Failed to close object proxy");
            anyhow::Ok(res)
        }
    }
}

pub async fn create_proxy(
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
            process::exit(1);
        }
    };
    let object_proxy = oprc_offload::proxy::ObjectProxy::new(session);
    object_proxy
}

pub async fn handle_obj_ops_zenoh(
    opt: &ObjectOperation,
    conn: &ConnectionArgs,
) {
    let object_proxy = create_proxy(conn).await;
    match opt {
        ObjectOperation::Set {
            cls_id,
            partition_id,
            id,
            byte_value,
        } => {
            let obj =
                crate::obj::util::parse_key_value_pairs(byte_value.clone());
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
            let object_proxy = create_proxy(conn).await;
            let meta = ObjMeta {
                cls_id: cls_id.clone(),
                partition_id: *partition_id as u32,
                object_id: *id,
            };
            let obj = match object_proxy.get_obj(&meta).await {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("Failed to get object: {:?}", e);
                    process::exit(1);
                }
            };
            match obj {
                Some(o) => {
                    o.pretty_print();
                }
                _ => {
                    println!("NONE");
                }
            }
        }
    }
}
