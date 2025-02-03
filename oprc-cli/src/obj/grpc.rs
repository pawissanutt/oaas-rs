use oprc_pb::{
    oprc_function_client::OprcFunctionClient, InvocationRequest,
    ObjectInvocationRequest,
};

use crate::{ConnectionArgs, InvokeOperation};

use super::util::extract_payload;

pub async fn invoke_fn(
    opt: &InvokeOperation,
    connect: &ConnectionArgs,
) -> anyhow::Result<oprc_pb::InvocationResponse> {
    let url = connect.grpc_url.clone().unwrap();
    let mut client = OprcFunctionClient::connect(url.clone()).await.unwrap();
    let payload = extract_payload(opt);
    let result = if let Some(oid) = opt.object_id {
        let result = client
            .invoke_obj(ObjectInvocationRequest {
                cls_id: opt.cls_id.clone(),
                partition_id: opt.partition_id as u32,
                fn_id: opt.fn_id.clone(),
                object_id: oid,
                payload,
                ..Default::default()
            })
            .await;
        result
    } else {
        let result = client
            .invoke_fn(InvocationRequest {
                cls_id: opt.cls_id.clone(),
                fn_id: opt.fn_id.clone(),
                payload,
                ..Default::default()
            })
            .await;
        result
    };

    anyhow::Ok(result?.into_inner())
}
