use std::collections::HashMap;

use bytes::Bytes;
use oprc_pb::{
    data_service_client::DataServiceClient, val_data::Data, ObjData,
    SetObjectRequest, SingleObjectRequest, ValData,
};
use prost::Message;

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
                format!("oprc/{}/{}/objects/{}", cls_id, partition_id, id);
            session
                .put(&key_expr, payload)
                .await
                .expect("Failed to put object");
            println!("set success");
        }
        ObjectOperation::Get {
            cls_id,
            partition_id,
            id,
        } => {
            let key_expr =
                format!("oprc/{}/{}/objects/{}", cls_id, partition_id, id);
            let get_result =
                session.get(&key_expr).await.expect("Failed to get object");
            let reply = get_result
                .recv_async()
                .await
                .expect("Failed to receive async reply");
            match reply.result() {
                Ok(sample) => {
                    let obj_data =
                        ObjData::decode(sample.payload().to_bytes().as_ref())
                            .expect("decode response failed");
                    print!("{:?}\n", obj_data);
                }
                Err(err) => {
                    print!("Failed to get object: {:?}", err);
                }
            }
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
