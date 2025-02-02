use std::process;

use oprc_pb::{ObjData, ObjMeta};

use crate::{ConnectionArgs, InvokeOperation, ObjectOperation};

mod grpc;
mod util;
mod z_api;

pub async fn handle_obj_ops(opt: &ObjectOperation) {
    handle_obj_ops_zenoh(opt).await;
}

pub async fn handle_invoke_ops(
    opt: &InvokeOperation,
    connect: &ConnectionArgs,
) {
    let res = if connect.grpc_url.is_some() {
        grpc::handle_invoke_ops(opt, connect).await
    } else {
        z_api::invoke_func(opt, connect).await
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

async fn handle_obj_ops_zenoh(opt: &ObjectOperation) {
    match opt {
        ObjectOperation::Set {
            cls_id,
            partition_id,
            id,
            byte_value,
            conn,
            ..
        } => {
            let object_proxy = z_api::create_proxy(conn).await;
            let obj = util::parse_key_value_pairs(byte_value.clone());
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
            conn,
            ..
        } => {
            let object_proxy = z_api::create_proxy(conn).await;
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

// async fn handle_obj_ops_grpc(opt: &ObjectOperation, connect: &ConnectionArgs) {
//     let mut client =
//         match DataServiceClient::connect(connect.grpc_url.clone().unwrap())
//             .await
//         {
//             Ok(c) => c,
//             Err(e) => {
//                 eprintln!("Failed to connect to gRPC server: {:?}", e);
//                 process::exit(1);
//             }
//         };
//     match opt {
//         ObjectOperation::Set {
//             cls_id,
//             partition_id,
//             id,
//             byte_value,
//             ..
//         } => {
//             let obj = parse_key_value_pairs(byte_value.clone());
//             let resp = match client
//                 .set(SetObjectRequest {
//                     cls_id: cls_id.clone(),
//                     partition_id: *partition_id as i32,
//                     object_id: *id,
//                     object: Some(ObjData {
//                         entries: obj,
//                         ..Default::default()
//                     }),
//                 })
//                 .await
//             {
//                 Ok(response) => response,
//                 Err(e) => {
//                     eprintln!("Failed to set object: {:?}", e);
//                     process::exit(1);
//                 }
//             };
//             print!("set success: {:?}\n", resp.into_inner());
//         }
//         ObjectOperation::Get {
//             cls_id,
//             partition_id,
//             id,
//             ..
//         } => {
//             let resp = match client
//                 .get(SingleObjectRequest {
//                     cls_id: cls_id.clone(),
//                     partition_id: *partition_id,
//                     object_id: *id,
//                 })
//                 .await
//             {
//                 Ok(response) => response,
//                 Err(e) => {
//                     eprintln!("Failed to get object: {:?}", e);
//                     process::exit(1);
//                 }
//             };
//             print!("{:?}\n", resp.into_inner());
//         }
//     }
// }
