use crate::error::GatewayError;
use axum::Extension;
use axum::extract::{Path, rejection::BytesRejection};
use axum::response::IntoResponse;
use bytes::Bytes;
use oprc_invoke::{Invoker, proxy::ObjectProxy, route::Routable};
use oprc_pb::{InvocationRequest, ObjData, ObjMeta, ObjectInvocationRequest};
use prost::DecodeError;
use tracing::warn;

type ConnMan = Invoker;

#[derive(serde::Deserialize, Debug)]
pub struct InvokeObjectPath {
    cls: String,
    pid: u16,
    oid: u64,
    func: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct ObjectPath {
    cls: String,
    pid: u32,
    oid: u64,
}

#[derive(serde::Deserialize, Debug)]
pub struct InvokeFunctionPath {
    cls: String,
    #[allow(dead_code)]
    pid: String,
    func: String,
}

#[axum::debug_handler]
pub async fn invoke_fn(
    Path(path): Path<InvokeFunctionPath>,
    Extension(cm): Extension<ConnMan>,
    body: Bytes,
) -> Result<Bytes, GatewayError> {
    let routable = Routable {
        cls: path.cls.clone(),
        func: path.func.clone(),
        partition: 0,
    };
    let result_conn = cm.get_conn(routable).await;
    match result_conn {
        Ok(mut conn) => {
            let req = InvocationRequest {
                cls_id: path.cls.clone(),
                fn_id: path.func.clone(),
                payload: body.to_vec(),
                ..Default::default()
            };
            let result = conn.invoke_fn(req).await;
            match result {
                Ok(resp) => {
                    let resp = resp.into_inner();
                    if let Some(playload) = resp.payload {
                        Ok(bytes::Bytes::from(playload))
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

#[axum::debug_handler]
pub async fn invoke_obj(
    Path(path): Path<InvokeObjectPath>,
    Extension(cm): Extension<ConnMan>,
    body: Bytes,
) -> Result<Bytes, GatewayError> {
    let routable = Routable {
        cls: path.cls.clone(),
        func: path.func.clone(),
        partition: path.pid,
    };
    let mut conn = cm.get_conn(routable).await?;
    let req = ObjectInvocationRequest {
        partition_id: path.pid as u32,
        object_id: path.oid,
        cls_id: path.cls.clone(),
        fn_id: path.func.clone(),
        payload: body.to_vec(),
        ..Default::default()
    };
    let result = conn.invoke_obj(req).await;
    match result {
        Ok(resp) => {
            let resp = resp.into_inner();
            if let Some(playload) = resp.payload {
                Ok(Bytes::from(playload))
            } else {
                Ok(Bytes::new())
            }
        }
        Err(e) => Err(GatewayError::from(e)),
    }
}

#[axum::debug_handler]
pub async fn get_obj(
    Path(path): Path<ObjectPath>,
    Extension(proxy): Extension<ObjectProxy>,
) -> Result<Protobuf<ObjData>, GatewayError> {
    // tracing::debug!("get object: {:?}", path);
    let obj = proxy
        .get_obj(&ObjMeta {
            cls_id: path.cls.clone(),
            partition_id: path.pid as u32,
            object_id: path.oid,
        })
        .await?;
    tracing::debug!("get object: {:?} {:?}", path, obj);
    if let Some(o) = obj {
        return Ok(Protobuf(o));
    } else {
        return Err(GatewayError::NoObj(
            path.cls.clone(),
            path.pid as u32,
            path.oid,
        ));
    }
}

#[axum::debug_handler]
pub async fn put_obj(
    Path(path): Path<ObjectPath>,
    Extension(proxy): Extension<ObjectProxy>,
    body: Protobuf<ObjData>,
) -> Result<(), GatewayError> {
    tracing::debug!("put object: {:?}", path);
    let meta = ObjMeta {
        cls_id: path.cls.clone(),
        partition_id: path.pid as u32,
        object_id: path.oid,
    };
    let mut obj = body.0;
    obj.metadata = Some(meta);
    proxy.set_obj(obj).await?;
    return Ok(());
}

pub struct Protobuf<T>(pub T);

impl<T> axum::response::IntoResponse for Protobuf<T>
where
    T: ::prost::Message,
{
    fn into_response(self) -> axum::response::Response {
        let mut buf = bytes::BytesMut::with_capacity(128);
        match &self.0.encode(&mut buf) {
            Ok(()) => buf.into_response(),
            Err(err) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
                    .into_response()
            }
        }
    }
}

// #[async_trait::async_trait]
impl<T, S> axum::extract::FromRequest<S> for Protobuf<T>
where
    T: prost::Message + Default,
    S: Send + Sync,
{
    type Rejection = ProtobufRejection;

    async fn from_request(
        req: axum::extract::Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let mut bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| ProtobufRejection::BytesRejection(e))?;

        match T::decode(&mut bytes) {
            Ok(value) => Ok(Protobuf(value)),
            Err(err) => Err(ProtobufRejection::ProtobufDecodeError(err)),
        }
    }
}

pub enum ProtobufRejection {
    BytesRejection(BytesRejection),
    ProtobufDecodeError(DecodeError),
}

impl IntoResponse for ProtobufRejection {
    fn into_response(self) -> axum::response::Response {
        match self {
            ProtobufRejection::BytesRejection(e) => e.into_response(),
            ProtobufRejection::ProtobufDecodeError(e) => {
                (http::StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
                    .into_response()
            }
        }
    }
}
