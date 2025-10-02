use crate::error::GatewayError;
use axum::Extension;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::response::Response;
use bytes::Bytes;
use http::StatusCode;
use oprc_grpc::{InvocationRequest, ObjData, ObjMeta, ObjectInvocationRequest};
use oprc_invoke::proxy::ObjectProxy;
use prost::Message;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

type ConnMan = ObjectProxy;

#[derive(serde::Deserialize, Debug)]
pub struct InvokeObjectPath {
    cls: String,
    pid: u16,
    oid: u64,
    func: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct ObjectPath {
    pub cls: String,
    pub pid: u32,
    pub oid: String, // raw segment (numeric or string)
}

// New extractor supporting either numeric or string object IDs.
// Route definition will still bind a single path segment; we attempt numeric parse first.
#[derive(Debug, Clone)]
pub enum ObjectIdVariant {
    Numeric(u64),
    String(String),
}

#[derive(Debug, Clone)]
pub struct FlexibleObjectPath {
    pub cls: String,
    pub pid: u32,
    pub oid: ObjectIdVariant,
}

impl FlexibleObjectPath {
    pub fn from_segments(cls: String, pid: u32, raw_id: String) -> Result<Self, GatewayError> {
        if let Ok(num) = raw_id.parse::<u64>() {
            return Ok(Self { cls, pid, oid: ObjectIdVariant::Numeric(num) });
        }
        // For Phase 1 we only normalize basic ASCII lowercase; reuse same pattern as backend.
        let norm = raw_id.to_ascii_lowercase();
        if !norm.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '.' | '_' | ':' | '-')) {
            return Err(GatewayError::UnknownError("invalid characters in string object id".into()));
        }
        if norm.is_empty() { return Err(GatewayError::UnknownError("empty object id".into())); }
        if norm.len() > 160 { return Err(GatewayError::UnknownError("object id too long".into())); }
        Ok(Self { cls, pid, oid: ObjectIdVariant::String(norm) })
    }
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
    Extension(proxy): Extension<ConnMan>,
    Extension(timeout): Extension<Duration>,
    Extension(retries): Extension<u32>,
    Extension(backoff): Extension<Duration>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let req = InvocationRequest {
        cls_id: path.cls.clone(),
        fn_id: path.func.clone(),
        payload: body.to_vec(),
        ..Default::default()
    };
    let mut req = req;
    if let Some(accept) = headers
        .get(http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
    {
        req.options.insert("accept".to_string(), accept.to_string());
    }
    if let Some(ct) = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        req.options
            .insert("content-type".to_string(), ct.to_string());
    }
    let mut attempt = 0u32;
    loop {
        let result =
            tokio::time::timeout(timeout, proxy.invoke_fn_with_req(&req)).await;
        match result {
            Ok(Ok(resp)) => {
                let resp_body = if let Some(playload) = resp.payload {
                    bytes::Bytes::from(playload)
                } else {
                    Bytes::new()
                };
                let mut resp_out = resp_body.into_response();
                // Prefer content-type returned from function response headers
                if let Some(ct) = resp.headers.get("content-type").cloned() {
                    if let Ok(hv) = http::HeaderValue::from_str(&ct) {
                        resp_out
                            .headers_mut()
                            .insert(http::header::CONTENT_TYPE, hv);
                    }
                } else if let Some(accept) = headers
                    .get(http::header::ACCEPT)
                    .and_then(|v| v.to_str().ok())
                {
                    // Fall back to client's accept when sensible
                    let ct = if accept.contains("application/json") {
                        "application/json"
                    } else {
                        "application/octet-stream"
                    };
                    resp_out.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static(ct),
                    );
                }
                return Ok(resp_out);
            }
            Ok(Err(e)) => {
                // retry only on RetrieveReplyErr
                if let oprc_invoke::proxy::ProxyError::RetrieveReplyErr(_) = e {
                    if attempt < retries {
                        attempt += 1;
                        sleep(backoff).await;
                        continue;
                    }
                }
                warn!("gateway error: {:?}", e);
                return Err(GatewayError::from(e));
            }
            Err(_) => {
                return Err(GatewayError::GrpcError(
                    tonic::Status::deadline_exceeded("timeout"),
                ));
            }
        }
    }
}

#[axum::debug_handler]
pub async fn invoke_obj(
    Path(path): Path<InvokeObjectPath>,
    Extension(proxy): Extension<ConnMan>,
    Extension(timeout): Extension<Duration>,
    Extension(retries): Extension<u32>,
    Extension(backoff): Extension<Duration>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GatewayError> {
    let req = ObjectInvocationRequest {
        partition_id: path.pid as u32,
        object_id: path.oid,
        cls_id: path.cls.clone(),
        fn_id: path.func.clone(),
        payload: body.to_vec(),
        ..Default::default()
    };
    let mut req = req;
    if let Some(accept) = headers
        .get(http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
    {
        req.options.insert("accept".to_string(), accept.to_string());
    }
    if let Some(ct) = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        req.options
            .insert("content-type".to_string(), ct.to_string());
    }
    let mut attempt = 0u32;
    loop {
        let result =
            tokio::time::timeout(timeout, proxy.invoke_obj_with_req(&req))
                .await;
        match result {
            Ok(Ok(resp)) => {
                let body = if let Some(playload) = resp.payload {
                    Bytes::from(playload)
                } else {
                    Bytes::new()
                };
                let mut out = body.into_response();
                if let Some(ct) = resp.headers.get("content-type").cloned() {
                    if let Ok(hv) = http::HeaderValue::from_str(&ct) {
                        out.headers_mut()
                            .insert(http::header::CONTENT_TYPE, hv);
                    }
                } else if let Some(accept) = headers
                    .get(http::header::ACCEPT)
                    .and_then(|v| v.to_str().ok())
                {
                    let ct = if accept.contains("application/json") {
                        "application/json"
                    } else {
                        "application/octet-stream"
                    };
                    out.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static(ct),
                    );
                }
                return Ok(out);
            }
            Ok(Err(e)) => {
                if let oprc_invoke::proxy::ProxyError::RetrieveReplyErr(_) = e {
                    if attempt < retries {
                        attempt += 1;
                        sleep(backoff).await;
                        continue;
                    }
                }
                return Err(GatewayError::from(e));
            }
            Err(_) => {
                return Err(GatewayError::GrpcError(
                    tonic::Status::deadline_exceeded("timeout"),
                ));
            }
        }
    }
}

#[axum::debug_handler]
pub async fn get_obj(
    Path(path): Path<ObjectPath>,
    Extension(proxy): Extension<ObjectProxy>,
    Extension(timeout): Extension<Duration>,
    Extension(retries): Extension<u32>,
    Extension(backoff): Extension<Duration>,
    headers: HeaderMap,
) -> Result<axum::response::Response, GatewayError> {
    // tracing::debug!("get object: {:?}", path);
    let mut attempt = 0u32;
    let obj = loop {
        let (object_id, object_id_str) = match path.oid.parse::<u64>() {
            Ok(num) => (num, None),
            Err(_) => {
                let norm = path.oid.to_ascii_lowercase();
                if norm.is_empty() {
                    return Err(GatewayError::InvalidObjectId("empty object id".into()));
                }
                if norm.len() > 160 {
                    return Err(GatewayError::InvalidObjectId("object id too long".into()));
                }
                if !norm.chars().all(|c| matches!(c,'a'..='z'|'0'..='9'|'.'|'_'|':'|'-')) {
                    return Err(GatewayError::InvalidObjectId("invalid characters in object id".into()));
                }
                (0, Some(norm))
            }
        };
        let meta = ObjMeta { cls_id: path.cls.clone(), partition_id: path.pid as u32, object_id, object_id_str };
        let fut = proxy.get_obj(&meta);
        match tokio::time::timeout(timeout, fut).await {
            Ok(Ok(o)) => break o,
            Ok(Err(e)) => {
                if let oprc_invoke::proxy::ProxyError::RetrieveReplyErr(_) = e {
                    if attempt < retries {
                        attempt += 1;
                        sleep(backoff).await;
                        continue;
                    }
                }
                return Err(GatewayError::from(e));
            }
            Err(_) => {
                return Err(GatewayError::GrpcError(
                    tonic::Status::deadline_exceeded("timeout"),
                ));
            }
        }
    };
    tracing::debug!("get object: {:?} {:?}", path, obj);
    if let Some(o) = obj {
        // If metadata is missing, treat as not found
        if o.metadata.is_none() {
            if let Ok(num) = path.oid.parse::<u64>() {
                return Err(GatewayError::NoObj(
                    path.cls.clone(),
                    path.pid as u32,
                    num,
                ));
            } else {
                return Err(GatewayError::NoObjStr(
                    path.cls.clone(),
                    path.pid as u32,
                    path.oid.clone(),
                ));
            }
        }
        // Choose JSON when client asks for it; default to protobuf
        let accept = headers
            .get(http::header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if accept.contains("application/json") {
            match serde_json::to_vec(&o) {
                Ok(body) => {
                    let mut resp = body.into_response();
                    resp.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/json"),
                    );
                    return Ok(resp);
                }
                Err(e) => {
                    return Ok((
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        e.to_string(),
                    )
                        .into_response());
                }
            }
        } else {
            let mut buf = bytes::BytesMut::with_capacity(128);
            if let Ok(_) = o.encode(&mut buf) {
                let mut resp = buf.into_response();
                let headers = resp.headers_mut();
                headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/x-protobuf"),
                );
                return Ok(resp);
            } else {
                // Fallback: return 500 if encoding fails (shouldn't happen)
                return Ok((
                    http::StatusCode::INTERNAL_SERVER_ERROR,
                    "encode error",
                )
                    .into_response());
            }
        }
    } else {
        if let Ok(num) = path.oid.parse::<u64>() {
            return Err(GatewayError::NoObj(
                path.cls.clone(),
                path.pid as u32,
                num,
            ));
        } else {
            return Err(GatewayError::NoObjStr(
                path.cls.clone(),
                path.pid as u32,
                path.oid.clone(),
            ));
        }
    }
}

#[axum::debug_handler]
pub async fn put_obj(
    Path(path): Path<ObjectPath>,
    Extension(proxy): Extension<ObjectProxy>,
    Extension(timeout): Extension<Duration>,
    Extension(retries): Extension<u32>,
    Extension(backoff): Extension<Duration>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(), GatewayError> {
    tracing::debug!("put object: {:?}", path);
    let (object_id, object_id_str) = match path.oid.parse::<u64>() {
        Ok(num) => (num, None),
        Err(_) => {
            let norm = path.oid.to_ascii_lowercase();
            if norm.is_empty() { return Err(GatewayError::InvalidObjectId("empty object id".into())); }
            if norm.len() > 160 { return Err(GatewayError::InvalidObjectId("object id too long".into())); }
            if !norm.chars().all(|c| matches!(c,'a'..='z'|'0'..='9'|'.'|'_'|':'|'-')) {
                return Err(GatewayError::InvalidObjectId("invalid characters in object id".into()));
            }
            (0, Some(norm))
        }
    };
    let meta = ObjMeta { cls_id: path.cls.clone(), partition_id: path.pid as u32, object_id, object_id_str };
    let content_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/x-protobuf");
    let mut obj = if content_type.starts_with("application/json") {
        serde_json::from_slice::<ObjData>(&body)
            .map_err(|e| GatewayError::UnknownError(e.to_string()))?
    } else if content_type.starts_with("application/x-protobuf")
        || content_type.starts_with("application/octet-stream")
    {
        ObjData::decode(body).map_err(|e| GatewayError::InvalidProtobuf(e))?
    } else {
        return Err(GatewayError::UnknownError(format!(
            "unsupported content-type: {}",
            content_type
        )));
    };
    obj.metadata = Some(meta);
    let mut attempt = 0u32;
    loop {
        match tokio::time::timeout(timeout, proxy.set_obj(obj.clone())).await {
            Ok(Ok(_)) => break,
            Ok(Err(e)) => {
                if let oprc_invoke::proxy::ProxyError::RetrieveReplyErr(_) = e {
                    if attempt < retries {
                        attempt += 1;
                        sleep(backoff).await;
                        continue;
                    }
                }
                return Err(GatewayError::from(e));
            }
            Err(_) => {
                return Err(GatewayError::GrpcError(
                    tonic::Status::deadline_exceeded("timeout"),
                ));
            }
        }
    }
    return Ok(());
}

#[axum::debug_handler]
pub async fn del_obj(
    Path(path): Path<ObjectPath>,
    Extension(proxy): Extension<ObjectProxy>,
    Extension(timeout): Extension<Duration>,
    Extension(retries): Extension<u32>,
    Extension(backoff): Extension<Duration>,
) -> Result<StatusCode, GatewayError> {
    let (object_id, object_id_str) = match path.oid.parse::<u64>() {
        Ok(num) => (num, None),
        Err(_) => {
            let norm = path.oid.to_ascii_lowercase();
            if norm.is_empty() { return Err(GatewayError::InvalidObjectId("empty object id".into())); }
            if norm.len() > 160 { return Err(GatewayError::InvalidObjectId("object id too long".into())); }
            if !norm.chars().all(|c| matches!(c,'a'..='z'|'0'..='9'|'.'|'_'|':'|'-')) {
                return Err(GatewayError::InvalidObjectId("invalid characters in object id".into()));
            }
            (0, Some(norm))
        }
    };
    let meta = ObjMeta { cls_id: path.cls.clone(), partition_id: path.pid as u32, object_id, object_id_str };
    let mut attempt = 0u32;
    loop {
        match tokio::time::timeout(timeout, proxy.del_obj(&meta)).await {
            Ok(Ok(_)) => break,
            Ok(Err(e)) => {
                if let oprc_invoke::proxy::ProxyError::RetrieveReplyErr(_) = e {
                    if attempt < retries {
                        attempt += 1;
                        sleep(backoff).await;
                        continue;
                    }
                }
                return Err(GatewayError::from(e));
            }
            Err(_) => {
                return Err(GatewayError::GrpcError(
                    tonic::Status::deadline_exceeded("timeout"),
                ));
            }
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
