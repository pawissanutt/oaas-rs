use std::error::Error;

use oprc_pb::{
    EmptyResponse, InvocationRequest, InvocationResponse, ObjData, ObjMeta,
    ObjectInvocationRequest,
};
use prost::Message;
use tonic::Status;
use zenoh::{
    bytes::ZBytes,
    key_expr::KeyExpr,
    qos::CongestionControl,
    query::{ConsolidationMode, QueryTarget, Reply},
};

use crate::serde::{decode, encode};

#[derive(thiserror::Error, Debug)]
pub enum ProxyError<T = EmptyResponse> {
    #[error("No queryable object found: {0}")]
    NoQueryable(Box<dyn Error + Send + Sync>),
    #[error("Failed to retrieve reply: {0}")]
    RetrieveReplyErr(Box<dyn Error + Send + Sync>),
    #[error("Got reply with error")]
    ReplyError(T),
    #[error("decode error: {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Require metadata")]
    RequireMetadata,
    #[error("Key error")]
    KeyErr(),
}

impl<T> Into<Status> for ProxyError<T> {
    fn into(self) -> Status {
        match self {
            ProxyError::NoQueryable(e) => {
                Status::not_found(format!("No queryable object found: {}", e))
            }
            ProxyError::RetrieveReplyErr(e) => {
                Status::internal(format!("Failed to retrieve reply: {}", e))
            }
            ProxyError::ReplyError(_) => {
                Status::internal("Got reply with error")
            }
            ProxyError::DecodeError(e) => {
                Status::internal(format!("Decode error: {}", e))
            }
            ProxyError::RequireMetadata => {
                Status::invalid_argument("Require metadata")
            }
            ProxyError::KeyErr() => Status::internal("Key error"),
        }
    }
}

#[derive(Clone)]
pub struct ProxyConfig {
    pub target_all: bool,
    pub conjection_control: CongestionControl,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            target_all: false,
            conjection_control: CongestionControl::Block,
        }
    }
}

#[derive(Clone)]
pub struct ObjectProxy {
    z_session: zenoh::Session,
    conf: ProxyConfig,
}

impl ProxyConfig {
    #[inline]
    fn get_target(&self) -> QueryTarget {
        if self.target_all {
            QueryTarget::All
        } else {
            QueryTarget::BestMatching
        }
    }
}

impl ObjectProxy {
    pub fn new(z_session: zenoh::Session) -> Self {
        Self {
            z_session,
            conf: Default::default(),
        }
    }

    pub fn with_config(z_session: zenoh::Session, conf: ProxyConfig) -> Self {
        Self { z_session, conf }
    }

    pub async fn get_obj(
        &self,
        meta: &ObjMeta,
    ) -> Result<Option<ObjData>, ProxyError> {
        let key_expr = format!(
            "oprc/{}/{}/objects/{}",
            meta.cls_id, meta.partition_id, meta.object_id
        );
        let data = self
            .call_zenoh(key_expr, None, |sample| {
                let payload = sample.payload();
                ObjData::decode(payload.to_bytes().as_ref())
                    .map_err(ProxyError::from)
                    .map(|o| if o.metadata.is_none() { None } else { Some(o) })
                // .map_err(ProxyError::DecodeError)
            })
            .await?;
        Ok(data)
    }

    pub async fn set_obj(
        &self,
        obj: ObjData,
    ) -> Result<EmptyResponse, ProxyError> {
        let key_expr = {
            if let Some(meta) = &obj.metadata {
                format!(
                    "oprc/{}/{}/objects/{}/set",
                    meta.cls_id, meta.partition_id, meta.object_id
                )
            } else {
                return Err(ProxyError::RequireMetadata);
            }
        };
        let payload = Some(ZBytes::from(obj.encode_to_vec()));
        return self
            .call_zenoh(key_expr, payload, |_| Ok(EmptyResponse::default()))
            .await;
    }

    pub async fn del_obj(&self, meta: ObjMeta) -> Result<(), ProxyError> {
        let key_expr = format!(
            "oprc/{}/{}/objects/{}",
            meta.cls_id, meta.partition_id, meta.object_id
        );
        self.z_session
            .delete(&key_expr)
            .await
            .map_err(|e| ProxyError::NoQueryable(e))?;
        Ok(())
    }

    pub async fn invoke_fn(
        &self,
        cls: &str,
        partition_id: u16,
        fn_id: &str,
        payload: Vec<u8>,
    ) -> Result<InvocationResponse, ProxyError> {
        let key_expr =
            format!("oprc/{}/{}/invokes/{}", cls, partition_id, fn_id);
        let req = InvocationRequest {
            cls_id: cls.to_string(),
            fn_id: fn_id.to_string(),
            payload,
            ..Default::default()
        };
        self.call_zenoh(key_expr, Some(encode(&req)), |sample| {
            decode(sample.payload()).map_err(|e| ProxyError::DecodeError(e))
        })
        .await
    }

    pub async fn invoke_fn_with_req(
        &self,
        req: &InvocationRequest,
    ) -> Result<InvocationResponse, ProxyError> {
        let key_expr = format!(
            "oprc/{}/{}/invokes/{}",
            req.cls_id, req.partition_id, req.fn_id
        );
        self.call_zenoh(key_expr, Some(encode(req)), |sample| {
            decode(sample.payload()).map_err(|e| ProxyError::DecodeError(e))
        })
        .await
    }

    pub async fn invoke_fn_raw<'a>(
        &self,
        key_expr: &KeyExpr<'a>,
        req: InvocationRequest,
    ) -> Result<InvocationResponse, ProxyError> {
        let reply = self.call_zenoh_raw(&key_expr, encode(&req)).await?;
        match reply.result() {
            Ok(sample) => {
                decode(sample.payload()).map_err(|e| ProxyError::DecodeError(e))
            }
            Err(reply_err) => decode(reply_err.payload())
                .map_err(|e| ProxyError::DecodeError(e)),
        }
    }

    pub async fn invoke_object_fn(
        &self,
        meta: &ObjMeta,
        fn_name: &str,
        payload: Vec<u8>,
    ) -> Result<InvocationResponse, ProxyError> {
        let key_expr = format!(
            "oprc/{}/{}/objects/{}/invokes/{}",
            meta.cls_id, meta.partition_id, meta.object_id, fn_name
        );
        let req = ObjectInvocationRequest {
            cls_id: meta.cls_id.to_string(),
            fn_id: fn_name.to_string(),
            partition_id: meta.partition_id,
            object_id: meta.object_id,
            payload,
            ..Default::default()
        };
        self.call_zenoh(key_expr, Some(encode(&req)), |sample| {
            decode(sample.payload()).map_err(|e| ProxyError::DecodeError(e))
        })
        .await
    }

    pub async fn invoke_obj_with_req(
        &self,
        req: &ObjectInvocationRequest,
    ) -> Result<InvocationResponse, ProxyError> {
        let key_expr = format!(
            "oprc/{}/{}/objects/{}/invokes/{}",
            req.cls_id, req.partition_id, req.object_id, req.fn_id
        );
        self.call_zenoh(key_expr, Some(encode(req)), |sample| {
            decode(sample.payload()).map_err(|e| ProxyError::DecodeError(e))
        })
        .await
    }

    pub async fn invoke_obj_fn_raw<'a>(
        &self,
        req: ObjectInvocationRequest,
    ) -> Result<InvocationResponse, ProxyError> {
        let key_expr = format!(
            "oprc/{}/{}/objects/{}/invokes/{}",
            req.cls_id, req.partition_id, req.object_id, req.fn_id
        )
        .try_into()
        .map_err(|_| ProxyError::KeyErr())?;
        let reply = self.call_zenoh_raw(&key_expr, encode(&req)).await?;
        match reply.result() {
            Ok(sample) => {
                decode(sample.payload()).map_err(|e| ProxyError::DecodeError(e))
            }
            Err(reply_err) => decode(reply_err.payload())
                .map_err(|e| ProxyError::DecodeError(e)),
        }
    }

    pub async fn call_zenoh<F, T>(
        &self,
        key_expr: String,
        payload: Option<ZBytes>,
        f: F,
    ) -> Result<T, ProxyError>
    where
        F: FnOnce(&zenoh::sample::Sample) -> Result<T, ProxyError>,
    {
        tracing::debug!("zenoh: GET {}", key_expr);
        let mut builder = self.z_session.get(&key_expr);
        if let Some(payload) = payload {
            builder = builder.payload(payload);
        }
        let (tx, rx) = flume::unbounded();
        let rx = builder
            .consolidation(ConsolidationMode::None)
            .congestion_control(self.conf.conjection_control.clone())
            .target(self.conf.get_target())
            // .callback(move |s| {
            //     let _ = tx.send(s);
            // })
            .with((tx, rx))
            .await
            .map_err(|e| ProxyError::NoQueryable(e))?;

        let data = match rx.recv_async().await {
            Ok(reply) => match reply.result() {
                Ok(sample) => f(sample)?,
                Err(_) => {
                    return Err(ProxyError::ReplyError(
                        EmptyResponse::default(),
                    ));
                }
            },
            Err(err) => {
                return Err(ProxyError::RetrieveReplyErr(Box::new(err)));
            }
        };
        Ok(data)
    }

    #[inline]
    pub async fn call_zenoh_raw<'a>(
        &self,
        key_expr: &KeyExpr<'a>,
        payload: ZBytes,
    ) -> Result<Reply, ProxyError> {
        // tracing::debug!("zenoh: GET {}", key_expr);
        let (tx, rx) = flume::bounded(16);
        self.z_session
            .get(key_expr)
            .payload(payload)
            .consolidation(ConsolidationMode::None)
            .congestion_control(CongestionControl::Block)
            .target(self.conf.get_target())
            .callback(move |s| {
                let _ = tx.send(s);
            })
            .await
            .map_err(|e| ProxyError::NoQueryable(e))?;
        rx.recv_async()
            .await
            .map_err(|e| ProxyError::RetrieveReplyErr(Box::new(e)))
    }

    pub async fn close(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.z_session.close().await
    }
}
