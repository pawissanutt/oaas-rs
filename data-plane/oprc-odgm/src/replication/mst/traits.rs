//! Core traits for MST replication

use async_trait::async_trait;

use super::types::{GenericLoadPageReq, GenericNetworkPage, GenericPagesResp};

/// Trait for abstracting network operations in MST replication
#[async_trait]
pub trait MstNetworking<T>: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the networking layer
    async fn start(&self) -> Result<(), Self::Error>;

    /// Stop the networking layer
    async fn stop(&self) -> Result<(), Self::Error>;

    /// Publish MST page ranges to peers
    async fn publish_pages(
        &self,
        owner: u64,
        pages: Vec<GenericNetworkPage>,
    ) -> Result<(), Self::Error>;

    /// Request pages from a peer
    async fn request_pages(
        &self,
        peer: u64,
        request: GenericLoadPageReq,
    ) -> Result<GenericPagesResp<T>, Self::Error>;
}
