#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Item not found: {0}")]
    NotFound(String),

    #[error("Item already exists: {0}")]
    AlreadyExists(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Storage backend error: {0}")]
    Backend(String),

    #[cfg(feature = "etcd")]
    #[error("etcd error: {0}")]
    Etcd(#[from] etcd_rs::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}
