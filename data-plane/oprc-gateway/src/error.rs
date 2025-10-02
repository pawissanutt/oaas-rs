use axum::response::IntoResponse;
use http::StatusCode;
use oprc_invoke::proxy::ProxyError as ZProxyError;
use tonic::Status;

#[derive(thiserror::Error, Debug)]
pub enum GatewayError {
    #[error("gRPC error: {0}")]
    GrpcError(#[from] Status),
    #[error("Parse ID error: {0}")]
    ParseIdError(#[from] ParseIdError),
    #[error("Invalid object id: {0}")]
    InvalidObjectId(String),
    #[error("Uri parsing error: {0}")]
    InvalidUrl(#[from] http::uri::InvalidUri),
    #[error("Invalid protobuf: {0}")]
    InvalidProtobuf(#[from] prost::DecodeError),
    #[error("No class {0} exists")]
    NoCls(String),
    #[error("No object exists in {0} with partition {1} and id {2}")]
    NoObj(String, u32, u64),
    #[error("No object exists in {0} with partition {1} and string id {2}")]
    NoObjStr(String, u32, String),
    #[error("No func {1} on class {0} exists")]
    NoFunc(String, String),
    #[error("No partition {1} on class {0} exists")]
    NoPartition(String, u16),
    #[error("Proxy error: {0}")]
    ProxyError(#[from] ZProxyError),
    #[error("Error: {0}")]
    UnknownError(String),
}

impl From<GatewayError> for tonic::Status {
    fn from(value: GatewayError) -> Self {
        match value {
            GatewayError::NoCls(_) => Status::not_found("not found class"),
            GatewayError::NoFunc(_, _) => Status::not_found("not found class"),
            GatewayError::NoObjStr(..) => Status::not_found("not found object"),
            GatewayError::GrpcError(s) => s,
            GatewayError::InvalidObjectId(msg) => Status::invalid_argument(msg),
            _ => Status::unknown(value.to_string()),
        }
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> axum::response::Response {
        use GatewayError::*;
        let (code, msg) = match &self {
            ParseIdError(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            InvalidUrl(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            InvalidProtobuf(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
            }
            InvalidObjectId(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            NoCls(_) | NoFunc(_, _) | NoPartition(_, _) | NoObj(_, _, _) | NoObjStr(_, _, _) => {
                (StatusCode::NOT_FOUND, self.to_string())
            }
            ProxyError(e) => match e {
                ZProxyError::NoQueryable(_) => {
                    (StatusCode::NOT_FOUND, self.to_string())
                }
                ZProxyError::RetrieveReplyErr(_) => {
                    (StatusCode::BAD_GATEWAY, self.to_string())
                }
                ZProxyError::ReplyError(_) => {
                    (StatusCode::BAD_GATEWAY, self.to_string())
                }
                ZProxyError::DecodeError(_) => {
                    (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
                }
                ZProxyError::RequireMetadata => {
                    (StatusCode::BAD_REQUEST, self.to_string())
                }
                ZProxyError::KeyErr() => {
                    (StatusCode::BAD_REQUEST, self.to_string())
                }
            },
            GrpcError(status) => {
                // For REST, map common gRPC status codes to HTTP
                match status.code() {
                    tonic::Code::NotFound => {
                        (StatusCode::NOT_FOUND, status.to_string())
                    }
                    tonic::Code::InvalidArgument => {
                        (StatusCode::BAD_REQUEST, status.to_string())
                    }
                    tonic::Code::DeadlineExceeded => {
                        (StatusCode::GATEWAY_TIMEOUT, status.to_string())
                    }
                    tonic::Code::Unavailable => {
                        (StatusCode::BAD_GATEWAY, status.to_string())
                    }
                    _ => (StatusCode::BAD_GATEWAY, status.to_string()),
                }
            }
            UnknownError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
        };
        let code_str = match &self {
            ParseIdError(_) => "PARSE_ID_ERROR",
            InvalidUrl(_) => "INVALID_URL",
            InvalidProtobuf(_) => "INVALID_PROtobuf",
            InvalidObjectId(_) => "INVALID_OBJECT_ID",
            NoCls(_) => "NO_CLASS",
            NoObj(_, _, _) => "NO_OBJECT",
            NoObjStr(_, _, _) => "NO_OBJECT",
            NoFunc(_, _) => "NO_FUNCTION",
            NoPartition(_, _) => "NO_PARTITION",
            ProxyError(ZProxyError::NoQueryable(_)) => "NO_QUERYABLE",
            ProxyError(ZProxyError::RetrieveReplyErr(_)) => {
                "ZENOH_RETRIEVE_ERR"
            }
            ProxyError(ZProxyError::ReplyError(_)) => "ZENOH_REPLY_ERR",
            ProxyError(ZProxyError::DecodeError(_)) => "DECODE_ERROR",
            ProxyError(ZProxyError::RequireMetadata) => "REQUIRE_METADATA",
            ProxyError(ZProxyError::KeyErr()) => "KEY_ERROR",
            GrpcError(status) => match status.code() {
                tonic::Code::NotFound => "NOT_FOUND",
                tonic::Code::InvalidArgument => "INVALID_ARGUMENT",
                tonic::Code::DeadlineExceeded => "DEADLINE_EXCEEDED",
                tonic::Code::Unavailable => "UNAVAILABLE",
                _ => "GRPC_ERROR",
            },
            UnknownError(_) => "UNKNOWN",
        };
        let body = serde_json::json!({
            "error": { "code": code_str, "message": msg }
        });
        let mut resp = (code, body.to_string()).into_response();
        resp.headers_mut().insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        resp
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseIdError {
    #[error("Invalid format. Expected '<partition_id>:<object_id>'.")]
    InvalidFormat,

    #[error("Failed to decode base32 partition_id.")]
    PartitionIdDecodeError,

    #[error("Failed to decode base32 object_id.")]
    ObjectIdDecodeError,

    #[error("Invalid partition_id length. Expected 2 bytes.")]
    InvalidPartitionIdLength,

    #[error("Invalid object_id length. Expected 8 bytes.")]
    InvalidObjectIdLength,
}
