use std::error::Error;

use tonic::Status;

type ShardId = u64;

#[derive(thiserror::Error, Debug)]
pub enum OdgmError {
    #[error("No shard `{0}` on current node")]
    NoShardFound(ShardId),
    #[error("No shard `{0:?}` on current node")]
    NoShardsFound(Vec<ShardId>),
    #[error("No collection `{0}` in cluster")]
    NoCollection(String),
    #[error("Invalid: {0}")]
    InvalidArgument(String),
    #[error("UnknownError: {0}")]
    UnknownError(#[from] Box<dyn Error + Send + Sync + 'static>),
    #[error("ZenohError: {0}")]
    ZenohError(Box<dyn Error + Send + Sync + 'static>),
    #[error("RPC Error: {0}")]
    RpcError(#[from] Status),
    #[error("Infallible")]
    Infallible,
}

impl OdgmError {
    pub fn from<E>(error: E) -> OdgmError
    where
        E: Error + Send + Sync + 'static,
    {
        OdgmError::UnknownError(Box::new(error))
    }
}

impl From<OdgmError> for tonic::Status {
    fn from(value: OdgmError) -> Self {
        match value {
            OdgmError::InvalidArgument(msg) => Status::invalid_argument(msg),
            OdgmError::UnknownError(e) => Status::from_error(e),
            OdgmError::RpcError(e) => e,
            _ => Status::not_found(value.to_string()),
        }
    }
}
