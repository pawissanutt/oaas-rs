use std::{io::Write, process};

use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjData, ObjMeta,
    ObjectInvocationRequest,
};
use oprc_invoke::{proxy::ObjectProxy, serde::encode};

use super::{
    resolve_class_id,
    util::{extract_payload, parse_key_value_pairs, parse_string_kv_pairs},
};
use crate::types::{
    ConnectionArgs, InvokeOperation, ObjectOperation, ResultOperation,
};

/// Handle object operations over Zenoh
pub async fn handle_obj_ops(opt: &ObjectOperation, conn: &ConnectionArgs) {
    let proxy = create_proxy(conn).await;

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
            set_object(
                &proxy,
                &resolved_cls_id,
                *partition_id,
                id,
                byte_value,
                str_value,
            )
            .await;
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
            get_object(
                &proxy,
                &resolved_cls_id,
                *partition_id as u32,
                id,
                key.as_ref(),
            )
            .await;
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
            let meta = ObjMeta {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as u32,
                object_id: Some(id.clone()),
            };
            match proxy.get_obj(&meta).await {
                Ok(Some(obj)) => {
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
                }
                Ok(None) => println!("NONE"),
                Err(e) => {
                    eprintln!("Failed to list string entries: {:?}", e);
                    process::exit(1);
                }
            }
        }
    }
}

/// Synchronous function invocation over Zenoh
pub async fn invoke_fn_sync(
    opt: &InvokeOperation,
    conn: &ConnectionArgs,
) -> anyhow::Result<InvocationResponse> {
    let cls_id = resolve_class_id(&opt.cls_id).await?;
    let payload = extract_payload(opt);
    let proxy = create_proxy(conn).await;

    match &opt.object_id {
        Some(object_id) => {
            let meta = create_obj_meta(&cls_id, opt.partition_id, object_id);
            proxy
                .invoke_object_fn(&meta, &opt.fn_id, payload)
                .await
                .map_err(|e| {
                    anyhow::anyhow!("Object method invocation failed: {}", e)
                })
        }
        None => proxy
            .invoke_fn(&cls_id, opt.partition_id, &opt.fn_id, payload)
            .await
            .map_err(|e| anyhow::anyhow!("Function invocation failed: {}", e)),
    }
}

/// Asynchronous function invocation over Zenoh
pub async fn invoke_fn_async(
    opt: &InvokeOperation,
    conn: &ConnectionArgs,
) -> anyhow::Result<String> {
    let payload = extract_payload(opt);
    let session = conn.open_zenoh().await;

    // Generate or use provided invocation ID
    let invocation_id = opt
        .invocation_id
        .clone()
        .unwrap_or_else(|| nanoid::nanoid!());

    // Build key expression and request payload
    let (key_expr, request_payload) =
        build_async_request(opt, &invocation_id, payload).await?;

    // Publish async invocation
    session.put(&key_expr, request_payload).await.map_err(|e| {
        anyhow::anyhow!("Failed to publish async invocation: {}", e)
    })?;

    Ok(invocation_id)
}

/// Get async invocation result
pub async fn get_async_result(
    opt: &ResultOperation,
    conn: &ConnectionArgs,
) -> anyhow::Result<Option<InvocationResponse>> {
    let session = conn.open_zenoh().await;

    let key_expr = build_result_key_expr(opt).await?;

    // Try to get the result
    let replies = session
        .get(&key_expr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query result: {}", e))?;

    // Check if we got a result
    match replies.recv_async().await {
        Ok(reply) => match reply.result() {
            Ok(sample) => {
                let response: InvocationResponse = prost::Message::decode(
                    sample.payload().to_bytes().as_ref(),
                )
                .map_err(|e| {
                    anyhow::anyhow!("Failed to decode response: {}", e)
                })?;
                Ok(Some(response))
            }
            Err(_) => Ok(None),
        },
        Err(_) => Ok(None),
    }
}

/// Create ObjectProxy with proper configuration
async fn create_proxy(conn: &ConnectionArgs) -> ObjectProxy {
    let session = conn.open_zenoh().await;
    ObjectProxy::new(session)
}

/// Create object metadata
fn create_obj_meta(
    cls_id: &str,
    partition_id: u16,
    object_id: &str,
) -> ObjMeta {
    ObjMeta {
        cls_id: cls_id.to_string(),
        partition_id: partition_id as u32,
        object_id: Some(object_id.to_string()),
    }
}

/// Set object with string id
async fn set_object(
    proxy: &ObjectProxy,
    cls_id: &str,
    partition_id: u16,
    object_id: &str,
    byte_values: &[String],
    str_values: &[String],
) {
    let mut entries = parse_key_value_pairs(byte_values.to_vec());
    entries.extend(parse_string_kv_pairs(str_values.to_vec()));
    let obj_data = ObjData {
        entries,
        metadata: Some(create_obj_meta(cls_id, partition_id, object_id)),
        ..Default::default()
    };
    match proxy.set_obj(obj_data).await {
        Ok(_) => println!("✓ Object set successfully"),
        Err(e) => {
            eprintln!("Failed to set object: {:?}", e);
            process::exit(1);
        }
    }
}

/// Get object using a string id
async fn get_object(
    proxy: &ObjectProxy,
    cls_id: &str,
    partition_id: u32,
    object_id: &str,
    key: Option<&String>,
) {
    let meta = ObjMeta {
        cls_id: cls_id.to_string(),
        partition_id,
        object_id: Some(object_id.to_string()),
    };

    match proxy.get_obj(&meta).await {
        Ok(Some(obj)) => {
            if let Some(field_key) = key {
                if let Some(entry) = obj.get_owned_entry(field_key) {
                    std::io::stdout()
                        .write_all(&entry)
                        .expect("Failed to write to stdout");
                    return;
                } else {
                    println!("Field {} not found", field_key);
                    return;
                }
            }
            obj.pretty_print();
        }
        Ok(None) => println!("Object not found"),
        Err(e) => {
            eprintln!("Failed to get object: {:?}", e);
            process::exit(1);
        }
    }
}

/// Build async request key expression and payload
async fn build_async_request(
    opt: &InvokeOperation,
    invocation_id: &str,
    payload: Vec<u8>,
) -> anyhow::Result<(String, zenoh::bytes::ZBytes)> {
    let cls_id = super::resolve_class_id(&opt.cls_id).await?;

    let key_expr = match &opt.object_id {
        Some(object_id) => {
            format!(
                "oprc/{}/{}/objects/{}/invokes/{}/async/{}",
                cls_id, opt.partition_id, object_id, opt.fn_id, invocation_id
            )
        }
        None => {
            format!(
                "oprc/{}/{}/invokes/{}/async/{}",
                cls_id, opt.partition_id, opt.fn_id, invocation_id
            )
        }
    };

    let request_payload = match &opt.object_id {
        Some(object_id) => {
            let req = ObjectInvocationRequest {
                cls_id: cls_id.clone(),
                partition_id: opt.partition_id as u32,
                object_id: Some(object_id.clone()),
                fn_id: opt.fn_id.clone(),
                payload,
                ..Default::default()
            };
            encode(&req)
        }
        None => {
            let req = InvocationRequest {
                cls_id: cls_id.clone(),
                partition_id: opt.partition_id as u32,
                fn_id: opt.fn_id.clone(),
                payload,
                ..Default::default()
            };
            encode(&req)
        }
    };

    Ok((key_expr, request_payload))
}

/// Build result key expression
async fn build_result_key_expr(
    opt: &ResultOperation,
) -> anyhow::Result<String> {
    let cls_id = super::resolve_class_id(&opt.cls_id).await?;

    let key_expr = match &opt.object_id {
        Some(object_id) => {
            format!(
                "oprc/{}/{}/objects/{}/results/{}/async/{}",
                cls_id,
                opt.partition_id,
                object_id,
                opt.fn_id,
                opt.invocation_id
            )
        }
        None => {
            format!(
                "oprc/{}/{}/results/{}/async/{}",
                cls_id, opt.partition_id, opt.fn_id, opt.invocation_id
            )
        }
    };

    Ok(key_expr)
}
