//! Networking implementations for MST replication with integrated handlers

use async_trait::async_trait;
use flare_zrpc::{
    bincode::BincodeMsgSerde, bincode::BincodeZrpcType, MsgSerde, ZrpcClient,
};
use merkle_search_tree::{diff::diff, MerkleSearchTree};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use zenoh::Session;

use oprc_dp_storage::{StorageBackend, StorageValue};

use super::error::MstError;
use super::traits::{
    MstNetworking, MstPageRequestHandler, MstPageUpdateHandler,
};
use super::types::{
    GenericLoadPageReq, GenericNetworkPage, GenericPageRangeMessage,
    GenericPagesResp, MstConfig, MstKey,
};

type GenericMessageSerde = BincodeMsgSerde<GenericPageRangeMessage>;
type GenericPageQueryType<T> =
    BincodeZrpcType<GenericLoadPageReq, GenericPagesResp<T>, ()>;

/// Implementation of page request handler
pub struct MstPageRequestHandlerImpl<T, S> {
    storage: Arc<S>,
    config: Arc<MstConfig<T>>,
}

impl<T, S> MstPageRequestHandlerImpl<T, S> {
    pub fn new(storage: Arc<S>, config: Arc<MstConfig<T>>) -> Self {
        Self { storage, config }
    }
}

#[async_trait]
impl<T, S> MstPageRequestHandler<T> for MstPageRequestHandlerImpl<T, S>
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
    S: StorageBackend + Send + Sync,
{
    async fn handle_page_request(
        &self,
        request: GenericLoadPageReq,
    ) -> Result<GenericPagesResp<T>, MstError> {
        let mut items = BTreeMap::new();

        for page in request.pages {
            // Scan storage for keys in the page range
            let _start_key = page.start_bounds.to_be_bytes();
            let _end_key = page.end_bounds.to_be_bytes();

            // Simple implementation: scan all and filter
            // In a real implementation, this would be more efficient with range queries
            let entries = self
                .storage
                .scan(&[])
                .await
                .map_err(|e| MstError(e.to_string()))?;

            for (key_bytes, value_bytes) in entries {
                if key_bytes.len() == 8 {
                    let key_u64 = u64::from_be_bytes(
                        key_bytes.as_slice().try_into().unwrap_or_default(),
                    );

                    if key_u64 >= page.start_bounds
                        && key_u64 <= page.end_bounds
                    {
                        let entry =
                            (self.config.deserialize)(value_bytes.as_slice())
                                .map_err(|e| MstError(e.to_string()))?;
                        items.insert(key_u64, entry);
                    }
                }
            }
        }

        Ok(GenericPagesResp { items })
    }
}

/// Implementation of page update handler
pub struct MstPageUpdateHandlerImpl<T, N, S> {
    storage: Arc<S>,
    mst: Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
    config: Arc<MstConfig<T>>,
    node_id: u64,
    networking: Arc<N>,
}

impl<T, N, S> MstPageUpdateHandlerImpl<T, N, S> {
    pub fn new(
        storage: Arc<S>,
        mst: Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
        config: Arc<MstConfig<T>>,
        node_id: u64,
        networking: Arc<N>,
    ) -> Self {
        Self {
            storage,
            mst,
            config,
            node_id,
            networking,
        }
    }
}

#[async_trait]
impl<T, N, S> MstPageUpdateHandler<T> for MstPageUpdateHandlerImpl<T, N, S>
where
    T: Clone
        + Send
        + Sync
        + Serialize
        + for<'de> Deserialize<'de>
        + std::hash::Hash
        + 'static,
    N: MstNetworking<T>,
    S: StorageBackend + Send + Sync,
{
    async fn handle_page_update(
        &self,
        owner: u64,
        pages: Vec<GenericNetworkPage>,
    ) -> Result<(), MstError> {
        if owner == self.node_id {
            return Ok(()); // Skip our own updates
        }

        let remote_pages = GenericNetworkPage::to_page_range(&pages);

        // Compare with local MST to find differences
        let req = {
            let mut mst_guard = self.mst.write().await;
            let _ = mst_guard.root_hash();
            let local_pages =
                mst_guard.serialise_page_ranges().unwrap_or(vec![]);
            let diff_pages = diff(local_pages, remote_pages);
            GenericLoadPageReq::from_diff(diff_pages)
        };

        if req.pages.is_empty() {
            return Ok(()); // No differences
        }

        tracing::debug!(
            "Requesting {} pages from owner {}",
            req.pages.len(),
            owner
        );

        // Request missing pages
        let resp = self
            .networking
            .request_pages(owner, req)
            .await
            .map_err(|e| MstError(format!("{:?}", e)))?;

        // Merge received data
        if !resp.items.is_empty() {
            tracing::info!(
                "Merging {} objects from {}",
                resp.items.len(),
                owner
            );

            for (key, remote_value) in resp.items {
                let key_bytes = key.to_be_bytes();

                // Resolve conflicts using merge function
                let final_value = if let Ok(Some(existing_bytes)) =
                    self.storage.get(&key_bytes).await
                {
                    let existing =
                        (self.config.deserialize)(existing_bytes.as_slice())
                            .map_err(|e| MstError(e.to_string()))?;
                    (self.config.merge_function)(
                        existing,
                        remote_value,
                        self.node_id,
                    )
                } else {
                    remote_value
                };

                // Store merged result
                let serialized = (self.config.serialize)(&final_value)
                    .map_err(|e| MstError(e.to_string()))?;

                self.storage
                    .put(&key_bytes, StorageValue::from(serialized))
                    .await
                    .map_err(|e| MstError(e.to_string()))?;

                // Update MST
                let mut mst_guard = self.mst.write().await;
                mst_guard.upsert(MstKey(key), &final_value);
            }
        }

        Ok(())
    }
}

/// Zenoh-based implementation of MstNetworking
pub struct ZenohMstNetworking<T> {
    prefix: String,
    zenoh_session: Session,
    page_request_handler:
        Arc<RwLock<Option<Arc<dyn MstPageRequestHandler<T> + Send + Sync>>>>,
    page_update_handler:
        Arc<RwLock<Option<Arc<dyn MstPageUpdateHandler<T> + Send + Sync>>>>,
}

impl<T> ZenohMstNetworking<T>
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    pub fn new(_shard_id: u64, prefix: String, zenoh_session: Session) -> Self {
        Self {
            prefix,
            zenoh_session,
            page_request_handler: Arc::new(RwLock::new(None)),
            page_update_handler: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl<T> MstNetworking<T> for ZenohMstNetworking<T>
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    type Error = MstError;

    async fn start(&self) -> Result<(), Self::Error> {
        // Start subscription for page updates
        let subscriber = self
            .zenoh_session
            .declare_subscriber(format!("{}/update-pages", self.prefix))
            .await
            .map_err(|e| MstError(e.to_string()))?;

        // Create ZRPC server for page requests
        // TODO: Implement ZRPC server setup

        // Handle page update messages
        let update_handler = self.page_update_handler.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(sample) = subscriber.recv_async().await {
                    let msg =
                        match GenericMessageSerde::from_zbyte(sample.payload())
                        {
                            Ok(msg) => msg,
                            Err(err) => {
                                tracing::error!(
                                    "Failed to decode page range message: {}",
                                    err
                                );
                                continue;
                            }
                        };

                    if let Some(handler) = update_handler.read().await.as_ref()
                    {
                        if let Err(err) = handler
                            .handle_page_update(msg.owner, msg.pages)
                            .await
                        {
                            tracing::error!(
                                "Failed to handle page update: {}",
                                err
                            );
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn stop(&self) -> Result<(), Self::Error> {
        // TODO: Implement cleanup
        Ok(())
    }

    async fn publish_pages(
        &self,
        owner: u64,
        pages: Vec<GenericNetworkPage>,
    ) -> Result<(), Self::Error> {
        let msg = GenericPageRangeMessage { owner, pages };
        let payload = GenericMessageSerde::to_zbyte(&msg)
            .map_err(|e| MstError(e.to_string()))?;

        self.zenoh_session
            .put(format!("{}/update-pages", self.prefix), payload)
            .await
            .map_err(|e| MstError(e.to_string()))?;

        Ok(())
    }

    async fn request_pages(
        &self,
        peer: u64,
        request: GenericLoadPageReq,
    ) -> Result<GenericPagesResp<T>, Self::Error> {
        // Create ZRPC client for requesting pages
        let rpc_client: ZrpcClient<GenericPageQueryType<T>> = ZrpcClient::new(
            format!("{}/page-query", self.prefix),
            self.zenoh_session.clone(),
        )
        .await;

        let resp = rpc_client
            .call_with_key(peer.to_string(), &request)
            .await
            .map_err(|e| MstError(format!("{:?}", e)))?;

        Ok(resp)
    }

    fn set_page_request_handler(
        &self,
        handler: Arc<dyn MstPageRequestHandler<T> + Send + Sync>,
    ) {
        tokio::spawn({
            let handler_ref = self.page_request_handler.clone();
            async move {
                *handler_ref.write().await = Some(handler);
            }
        });
    }

    fn set_page_update_handler(
        &self,
        handler: Arc<dyn MstPageUpdateHandler<T> + Send + Sync>,
    ) {
        tokio::spawn({
            let handler_ref = self.page_update_handler.clone();
            async move {
                *handler_ref.write().await = Some(handler);
            }
        });
    }
}

/// Mock networking implementation for testing
#[cfg(test)]
pub struct MockMstNetworking<T> {
    _phantom: std::marker::PhantomData<T>,
}

#[cfg(test)]
impl<T> MockMstNetworking<T> {
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
#[async_trait]
impl<T> MstNetworking<T> for MockMstNetworking<T>
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
{
    type Error = MstError;

    async fn start(&self) -> Result<(), Self::Error> {
        // No-op for testing
        Ok(())
    }

    async fn stop(&self) -> Result<(), Self::Error> {
        // No-op for testing
        Ok(())
    }

    async fn publish_pages(
        &self,
        _owner: u64,
        _pages: Vec<GenericNetworkPage>,
    ) -> Result<(), Self::Error> {
        // No-op for testing
        Ok(())
    }

    async fn request_pages(
        &self,
        _peer: u64,
        _request: GenericLoadPageReq,
    ) -> Result<GenericPagesResp<T>, Self::Error> {
        // Return empty response for testing
        Ok(GenericPagesResp {
            items: BTreeMap::new(),
        })
    }

    fn set_page_request_handler(
        &self,
        _handler: Arc<dyn MstPageRequestHandler<T> + Send + Sync>,
    ) {
        // No-op for testing
    }

    fn set_page_update_handler(
        &self,
        _handler: Arc<dyn MstPageUpdateHandler<T> + Send + Sync>,
    ) {
        // No-op for testing
    }
}
