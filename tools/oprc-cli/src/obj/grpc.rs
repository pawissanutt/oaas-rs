use std::io::Write;

use oprc_grpc::{
    InvocationRequest, ObjData, ObjectInvocationRequest, SetObjectRequest,
    SingleObjectRequest,
};
use oprc_grpc::{
    data_service_client::DataServiceClient,
    oprc_function_client::OprcFunctionClient,
};

use crate::types::{ConnectionArgs, InvokeOperation, ObjectOperation};

use super::{
    resolve_class_id,
    util::{extract_payload, parse_key_value_pairs, parse_string_kv_pairs},
};

pub async fn invoke_fn(
    opt: &InvokeOperation,
    connect: &ConnectionArgs,
) -> anyhow::Result<oprc_grpc::InvocationResponse> {
    let cls_id = resolve_class_id(&opt.cls_id).await?;
    let url = connect.grpc_url.clone().unwrap();
    let mut client = OprcFunctionClient::connect(url.clone()).await.unwrap();
    let payload = extract_payload(opt);
    let result = if let Some(oid) = opt.object_id {
        let result = client
            .invoke_obj(ObjectInvocationRequest {
                cls_id,
                partition_id: opt.partition_id as u32,
                fn_id: opt.fn_id.clone(),
                object_id: oid,
                payload: payload.into(),
                ..Default::default()
            })
            .await;
        result
    } else {
        let result = client
            .invoke_fn(InvocationRequest {
                cls_id,
                fn_id: opt.fn_id.clone(),
                payload: payload.into(),
                ..Default::default()
            })
            .await;
        result
    };

    anyhow::Ok(result?.into_inner())
}

pub async fn handle_obj_ops(opt: &ObjectOperation, conn: &ConnectionArgs) {
    let mut client = create_client(conn)
        .await
        .expect("Failed to create gRPC client");
    match opt {
        ObjectOperation::Set {
            cls_id,
            partition_id,
            id,
            byte_value,
            str_value,
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let obj = ObjData {
                metadata: None,
                entries: parse_key_value_pairs(byte_value.clone()),
                event: None,
                entries_str: parse_string_kv_pairs(str_value.clone()),
            };
            let req = SetObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as i32,
                object_id: *id,
                object: Some(obj),
                object_id_str: None,
            };
            client.set(req).await.expect("Failed to set object data");
        }
        ObjectOperation::Get {
            cls_id,
            partition_id,
            id,
            key,
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let req = SingleObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as u32,
                object_id: *id,
                object_id_str: None,
            };
            let resp = client.get(req).await;
            let obj = match resp {
                Ok(resp) => {
                    let inner = resp.into_inner();
                    inner.obj
                }
                Err(e) => {
                    eprintln!("Failed to get object: {:?}", e);
                    std::process::exit(1);
                }
            };
            match obj {
                Some(o) => {
                    if let Some(k) = key {
                        if let Some(item) = o.get_owned_entry(*k) {
                            std::io::stdout()
                                .write_all(&item)
                                .expect("Failed to write to stdout");
                        }
                    } else {
                        o.pretty_print();
                    }
                }
                _ => {
                    println!("NONE");
                }
            }
        }
        ObjectOperation::SetStr {
            cls_id,
            partition_id,
            object_id_str,
            byte_value,
            str_value,
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let obj = ObjData {
                metadata: None,
                entries: parse_key_value_pairs(byte_value.clone()),
                event: None,
                entries_str: parse_string_kv_pairs(str_value.clone()),
            };
            let req = SetObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as i32,
                object_id: 0, // ignored when object_id_str present
                object: Some(obj),
                object_id_str: Some(object_id_str.clone()),
            };
            client
                .set(req)
                .await
                .expect("Failed to set string-id object data");
        }
        ObjectOperation::GetStr {
            cls_id,
            partition_id,
            object_id_str,
            key,
            key_str,
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let req = SingleObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as u32,
                object_id: 0, // ignored for string id
                object_id_str: Some(object_id_str.clone()),
            };
            let resp = client.get(req).await;
            let obj = match resp {
                Ok(resp) => resp.into_inner().obj,
                Err(e) => {
                    eprintln!("Failed to get object: {:?}", e);
                    std::process::exit(1);
                }
            };
            match obj {
                Some(o) => {
                    if let Some(k) = key {
                        if let Some(item) = o.get_owned_entry(*k) {
                            std::io::stdout()
                                .write_all(&item)
                                .expect("Failed to write to stdout");
                            return;
                        }
                    }
                    if let Some(ks) = key_str {
                        if let Some(val) = o.entries_str.get(ks) {
                            std::io::stdout()
                                .write_all(&val.data)
                                .expect("Failed to write to stdout");
                            return;
                        } else {
                            eprintln!("String key '{}' not found", ks);
                            std::process::exit(1);
                        }
                    }
                    o.pretty_print();
                }
                None => println!("NONE"),
            }
        }
    };
}

pub async fn create_client(
    connect: &ConnectionArgs,
) -> anyhow::Result<DataServiceClient<tonic::transport::Channel>> {
    let url = connect.grpc_url.clone().unwrap();
    let client = DataServiceClient::connect(url).await?;
    Ok(client)
}
