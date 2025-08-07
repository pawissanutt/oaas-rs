//! Core traits for MST replication

use async_trait::async_trait;
use std::sync::Arc;

use super::error::MstError;
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

    /// Set the callback for handling incoming page requests
    fn set_page_request_handler(
        &self,
        handler: Arc<dyn MstPageRequestHandler<T> + Send + Sync>,
    );

    /// Set the callback for handling incoming page updates
    fn set_page_update_handler(
        &self,
        handler: Arc<dyn MstPageUpdateHandler<T> + Send + Sync>,
    );
}

/// Handler for incoming page requests
#[async_trait]
pub trait MstPageRequestHandler<T>: Send + Sync {
    async fn handle_page_request(
        &self,
        request: GenericLoadPageReq,
    ) -> Result<GenericPagesResp<T>, MstError>;
}

/// Handler for incoming page updates
#[async_trait]
pub trait MstPageUpdateHandler<T>: Send + Sync {
    async fn handle_page_update(
        &self,
        owner: u64,
        pages: Vec<GenericNetworkPage>,
    ) -> Result<(), MstError>;
}
