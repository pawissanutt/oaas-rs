use std::collections::HashMap;

use oprc_pb::{
    data_service_client::DataServiceClient, val_data::Data, ObjData,
    SetObjectRequest, SingleObjectRequest, ValData,
};
use prost::Message;
use zenoh::{
    bytes::ZBytes,
    query::{ConsolidationMode, QueryTarget},
};

use crate::{ConnectionArgs, ObjectOperation};

pub async fn handle_obj_ops(opt: &ObjectOperation, connect: &ConnectionArgs) {
    if connect.zenoh_peer.is_some() {
        handle_obj_ops_zenoh(opt, connect).await;
    } else {
        handle_obj_ops_grpc(opt, connect).await;
    }
}

async fn handle_obj_ops_zenoh(opt: &ObjectOperation, connect: &ConnectionArgs) {
    let config = oprc_zenoh::OprcZenohConfig {
        peers: connect.zenoh_peer.clone(),
        zenoh_port: 0,
        mode: zenoh_config::WhatAmI::Client,
        ..Default::default()
    }
    .create_zenoh();
    let session = zenoh::open(config)
        .await
        .expect("Failed to open Zenoh session");
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
                ..Default::default()
            };
            let payload = ObjData::encode_to_vec(&obj_data);
            let key_expr =
                format!("oprc/{}/{}/objects/{}/set", cls_id, partition_id, id);
            let get_result = session
                .get(&key_expr)
                .consolidation(ConsolidationMode::None)
                .target(QueryTarget::All)
                .payload(payload)
                .await
                .expect("Failed to set object");
            let resp = handle_get_result(get_result).await;
            if let Some(bytes) = resp {
                let obj_data =
                    oprc_pb::EmptyResponse::decode(bytes.to_bytes().as_ref())
                        .unwrap();
                print!("Set Successful: {:?}\n", obj_data);
            }
        }
        ObjectOperation::Get {
            cls_id,
            partition_id,
            id,
        } => {
            let key_expr =
                format!("oprc/{}/{}/objects/{}", cls_id, partition_id, id);
            let get_result = session
                .get(&key_expr)
                .consolidation(ConsolidationMode::None)
                .target(QueryTarget::All)
                .await
                .expect("Failed to get object");
            let resp = handle_get_result(get_result).await;
            if let Some(bytes) = resp {
                let obj_data =
                    ObjData::decode(bytes.to_bytes().as_ref()).unwrap();
                print!("{:?}\n", obj_data);
            }
        }
    }
}

async fn handle_get_result(
    get_result: zenoh::handlers::FifoChannelHandler<zenoh::query::Reply>,
) -> Option<ZBytes> {
    let reply = get_result
        .recv_async()
        .await
        .expect("Failed to receive async reply");
    match reply.result() {
        Ok(sample) => Some(sample.payload().clone()),
        Err(err) => {
            print!("Failed to get object: {:?}", err);
            None
        }
    }
}

async fn handle_obj_ops_grpc(opt: &ObjectOperation, connect: &ConnectionArgs) {
    let mut client = DataServiceClient::connect(connect.server_url.clone())
        .await
        .expect("Failed to connect to gRPC server");
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
                .await
                .expect("Failed to set object");
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
                .await
                .expect("Failed to get object");
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
