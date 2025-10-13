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
    util::{extract_payload, parse_key_value_pairs},
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
        } => {
            let resolved_cls_id = resolve_class_id(cls_id)
                .await
                .expect("Failed to resolve class ID");
            let obj = ObjData {
                metadata: None,
                entries: parse_key_value_pairs(byte_value.clone()),
                event: None,
            };
            let req = SetObjectRequest {
                cls_id: resolved_cls_id,
                partition_id: *partition_id as i32,
                object_id: *id,
                object: Some(obj),
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
    };
}

pub async fn create_client(
    connect: &ConnectionArgs,
) -> anyhow::Result<DataServiceClient<tonic::transport::Channel>> {
    let url = connect.grpc_url.clone().unwrap();
    let client = DataServiceClient::connect(url).await?;
    Ok(client)
}
