use thiserror::Error;

/// Main error type for storage operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Not found")]
    NotFound,

    #[error("Already exists")]
    AlreadyExists,

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

impl StorageError {
    pub fn serialization<T: ToString>(msg: T) -> Self {
        Self::Serialization(msg.to_string())
    }

    pub fn transaction<T: ToString>(msg: T) -> Self {
        Self::Transaction(msg.to_string())
    }

    pub fn backend<T: ToString>(msg: T) -> Self {
        Self::Backend(msg.to_string())
    }

    pub fn configuration<T: ToString>(msg: T) -> Self {
        Self::Configuration(msg.to_string())
    }

    pub fn invalid_operation<T: ToString>(msg: T) -> Self {
        Self::InvalidOperation(msg.to_string())
    }
}

/// Result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;
