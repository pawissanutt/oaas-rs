use thiserror::Error;

/// Client-related errors
#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    // #[error("Invalid URL: {0}")]
    // InvalidUrl(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl ClientError {
    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::ConfigError(msg.into())
    }

    // pub fn invalid_url(msg: impl Into<String>) -> Self {
    //     Self::InvalidUrl(msg.into())
    // }

    pub fn api_error(status: u16, message: impl Into<String>) -> Self {
        Self::ApiError {
            status,
            message: message.into(),
        }
    }
}
