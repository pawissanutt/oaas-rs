use std::error::Error;

use anyerror::AnyError;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum Infallible {}

#[derive(thiserror::Error, Debug)]
pub enum ZrpcError<E = Infallible> {
    #[error("connection error: {0}")]
    ConnectionError(#[from] Box<dyn Error + Send + Sync>),
    #[error("encode error: {0}")]
    EncodeError(AnyError),
    #[error("decode error: {0}")]
    DecodeError(AnyError),
    #[error("server error: {0}")]
    ServerSystemError(ZrpcSystemError),
    #[error("No quaryable target (sender dropped)")]
    SenderDropped(#[from] flume::RecvError),
    #[error("app error: {0}")]
    AppError(E),
}

#[derive(
    thiserror::Error, serde::Serialize, serde::Deserialize, Clone, Debug,
)]
pub enum ZrpcServerError<E> {
    #[error("app error: {0}")]
    AppError(E),
    #[error("system error: {0}")]
    SystemError(ZrpcSystemError),
}

#[derive(
    thiserror::Error, serde::Serialize, serde::Deserialize, Clone, Debug,
)]
pub enum ZrpcSystemError {
    #[error("decode error: {0}")]
    DecodeError(anyerror::AnyError),
    #[error("encode error: {0}")]
    EncodeError(anyerror::AnyError),
}
