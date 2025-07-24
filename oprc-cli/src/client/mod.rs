mod error;
mod http;

pub use error::*;
pub use http::*;

use crate::config::ContextConfig;

/// Client factory for creating API clients
pub struct ClientFactory;

impl ClientFactory {
    /// Create an HTTP client for the given context
    pub fn create_http_client(context: &ContextConfig) -> Result<HttpClient, ClientError> {
        HttpClient::new(context)
    }
}
