use std::io::Write;

use base64::prelude::*;
use oprc_grpc::{
    InvocationRequest, ListObjectsRequest, ObjData, ObjectInvocationRequest,
    SetObjectRequest, SingleObjectRequest,
};
use oprc_grpc::{
    data_service_client::DataServiceClient,
    oprc_function_client::OprcFunctionClient,
};
use tokio_stream::StreamExt;

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
    let result = if let Some(oid) = &opt.object_id {
        let result = client
            .invoke_obj(ObjectInvocationRequest {
                cls_id,
                partition_id: opt.partition_id as u32,
                fn_id: opt.fn_id.clone(),
                object_id: Some(oid.clone()),
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
    let mut client = match create_client(conn).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to connect to gRPC data service: {}", e);
            eprintln!(
                "Hint: ensure the OPRC gateway/data-plane is running and consider providing --grpc-url"
            );
            std::process::exit(1);
        }
    };
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
            let mut entries = parse_key_value_pairs(byte_value.clone());
            entries.extend(parse_string_kv_pairs(str_value.clone()));
            println!("DEBUG: entries to set: {:?}", entries.keys());
            let obj = ObjData {
                metadata: None,
                entries,
                event: None,
            };
            let req = SetObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as i32,
                object: Some(obj),
                object_id: Some(id.clone()),
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
                object_id: Some(id.clone()),
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
                        if let Some(item) = o.get_owned_entry(k) {
                            std::io::stdout()
                                .write_all(&item)
                                .expect("Failed to write to stdout");
                            return;
                        } else {
                            eprintln!("Key '{}' not found", k);
                            std::process::exit(1);
                        }
                    }
                    o.pretty_print();
                }
                None => println!("NONE"),
            }
        }
        ObjectOperation::ListStr {
            cls_id,
            partition_id,
            id,
            with_values,
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let req = SingleObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as u32,
                object_id: Some(id.clone()),
            };
            let resp = client.get(req).await;
            match resp {
                Ok(resp) => {
                    if let Some(obj) = resp.into_inner().obj {
                        if *with_values {
                            for (k, v) in obj.entries.iter() {
                                println!(
                                    "{}={}",
                                    k,
                                    String::from_utf8_lossy(&v.data)
                                );
                            }
                        } else {
                            for k in obj.entries.keys() {
                                println!("{}", k);
                            }
                        }
                    } else {
                        eprintln!("Object not found");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list object keys: {:?}", e);
                }
            }
        }
        ObjectOperation::List {
            cls_id,
            partition_id,
            prefix,
            limit,
            cursor,
            json,
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let cursor_bytes = cursor.as_ref().map(|c| {
                BASE64_URL_SAFE_NO_PAD
                    .decode(c)
                    .expect("Invalid base64 cursor")
            });
            let req = ListObjectsRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id,
                object_id_prefix: prefix.clone(),
                limit: *limit,
                cursor: cursor_bytes,
            };
            let resp = client.list_objects(req).await;
            match resp {
                Ok(stream) => {
                    let mut stream = stream.into_inner();
                    let mut objects = Vec::new();
                    let mut last_cursor: Option<Vec<u8>> = None;
                    while let Some(result) = stream.next().await {
                        match result {
                            Ok(envelope) => {
                                last_cursor = envelope.next_cursor.clone();
                                objects.push(envelope);
                            }
                            Err(e) => {
                                eprintln!("Stream error: {:?}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    if *json {
                        let output: Vec<_> = objects
                            .iter()
                            .map(|o| {
                                serde_json::json!({
                                    "object_id": o.object_id,
                                    "version": o.version,
                                    "entry_count": o.entry_count,
                                })
                            })
                            .collect();
                        let result = serde_json::json!({
                            "objects": output,
                            "count": objects.len(),
                            "next_cursor": last_cursor.as_ref().map(|c| BASE64_URL_SAFE_NO_PAD.encode(c)),
                        });
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&result).unwrap()
                        );
                    } else {
                        for obj in &objects {
                            println!(
                                "{}  v{}  entries={}",
                                obj.object_id, obj.version, obj.entry_count
                            );
                        }
                        println!("--- {} object(s) ---", objects.len());
                        if let Some(c) = &last_cursor {
                            println!(
                                "Next cursor: {}",
                                BASE64_URL_SAFE_NO_PAD.encode(c)
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list objects: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

pub async fn create_client(
    connect: &ConnectionArgs,
) -> anyhow::Result<DataServiceClient<tonic::transport::Channel>> {
    let url = connect.grpc_url.clone().unwrap();
    let client = DataServiceClient::connect(url).await?;
    Ok(client)
}
