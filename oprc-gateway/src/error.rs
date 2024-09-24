use axum::response::IntoResponse;
use http::StatusCode;
use tonic::Status;

#[derive(thiserror::Error, Debug)]
pub enum GatewayError {
    #[error("gRPC error: {0}")]
    GrpcError(#[from] Status),
    #[error("gRPC error: {0}")]
    GrpcConnectError(#[from] tonic::transport::Error),
    #[error("Parse ID error: {0}")]
    ParseIdError(#[from] ParseIdError),
    #[error("Uri parsing error: {0}")]
    InvalidUrl(#[from] http::uri::InvalidUri),
    #[error("Timeout")]
    Timeout,
    #[error("BadConn")]
    BadConn,
    #[error("PoolClosed")]
    PoolClosed,
    #[error("No class {0} exists")]
    NoCls(String),
    #[error("No func {1} on class {0} exists")]
    NoFunc(String, String),
    #[error("Error: {0}")]
    UnknownError(String),
}

impl<E: std::error::Error> From<mobc::Error<E>> for GatewayError {
    fn from(value: mobc::Error<E>) -> Self {
        match value {
            mobc::Error::Inner(e) => GatewayError::UnknownError(e.to_string()),
            mobc::Error::Timeout => GatewayError::Timeout,
            mobc::Error::BadConn => GatewayError::BadConn,
            mobc::Error::PoolClosed => GatewayError::PoolClosed,
        }
    }
}

impl From<GatewayError> for tonic::Status {
    fn from(value: GatewayError) -> Self {
        match value {
            GatewayError::NoCls(_) => Status::not_found("not found class"),
            GatewayError::NoFunc(_, _) => Status::not_found("not found class"),
            GatewayError::GrpcError(s) => s,
            GatewayError::GrpcConnectError(e) => {
                Status::unavailable(e.to_string())
            }
            GatewayError::Timeout => Status::deadline_exceeded("timeout"),
            _ => Status::unknown(value.to_string()),
        }
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::BAD_GATEWAY, self.to_string()).into_response()
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
