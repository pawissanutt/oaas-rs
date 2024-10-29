use crate::error::GatewayError;
use crate::id::parse_id;
use crate::route::Routable;
use crate::rpc::RpcManager;
use axum::extract::Path;
use axum::Extension;
use bytes::Bytes;
use oprc_common::conn::ConnManager;
use oprc_pb::{InvocationRequest, ObjectInvocationRequest};
use std::sync::Arc;
use tracing::warn;

type ConnMan = Arc<ConnManager<Routable, RpcManager>>;

#[derive(serde::Deserialize)]
pub struct ObjectPathParams {
    cls: String,
    oid: String,
    func: String,
}

#[derive(serde::Deserialize)]
pub struct FunctionPathParams {
    cls: String,
    func: String,
}

pub async fn invoke_fn(
    Path(path): Path<FunctionPathParams>,
    Extension(cm): Extension<ConnMan>,
    body: Bytes,
) -> Result<Bytes, GatewayError> {
    let routable = Routable {
        cls: path.cls.clone(),
        func: path.func.clone(),
        partition: 0,
    };
    let result_conn = cm.get(routable).await;
    match result_conn {
        Ok(mut conn) => {
            let req = InvocationRequest {
                cls_id: path.cls.clone(),
                fn_id: path.func.clone(),
                payload: body,
                ..Default::default()
            };
            let result = conn.invoke_fn(req).await;
            match result {
                Ok(resp) => {
                    let resp = resp.into_inner();
                    if let Some(playload) = resp.payload {
                        Ok(playload)
                    } else {
                        Ok(Bytes::new())
                    }
                }
                Err(e) => {
                    warn!("gateway error: {:?}", e);
                    Err(GatewayError::from(e))
                }
            }
        }
        Err(e) => {
            // warn!("gateway pool error: {:?}", e);
            Err(e.into())
        }
    }
}

pub async fn invoke_obj(
    Path(path): Path<ObjectPathParams>,
    Extension(cm): Extension<ConnMan>,
    body: Bytes,
) -> Result<Bytes, GatewayError> {
    let (pid, oid) = parse_id(&path.oid)?;
    let routable = Routable {
        cls: path.cls.clone(),
        func: path.func.clone(),
        partition: pid,
    };
    let mut conn = cm.get(routable).await?;
    let req = ObjectInvocationRequest {
        partition_id: pid as i32,
        object_id: oid,
        cls_id: path.cls.clone(),
        fn_id: path.func.clone(),
        payload: body,
        ..Default::default()
    };
    let result = conn.invoke_obj(req).await;
    match result {
        Ok(resp) => {
            let resp = resp.into_inner();
            if let Some(playload) = resp.payload {
                Ok(playload)
            } else {
                Ok(Bytes::new())
            }
        }
        Err(e) => Err(GatewayError::from(e)),
    }
}
