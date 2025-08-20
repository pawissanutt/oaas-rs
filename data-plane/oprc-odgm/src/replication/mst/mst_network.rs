//! Networking implementations for MST replication with integrated handlers

use async_trait::async_trait;
use flare_zrpc::{
    MsgSerde, ZrpcClient, ZrpcServiceHander,
    bincode::BincodeMsgSerde,
    bincode::BincodeZrpcType,
    server::{ServerConfig, ZrpcService},
};
use merkle_search_tree::{MerkleSearchTree, diff::diff};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use zenoh::Session;

use oprc_dp_storage::{StorageBackend, StorageValue};

use super::error::MstError;
use super::mst_traits::MstNetworking;
use super::types::{
    GenericLoadPageReq, GenericNetworkPage, GenericPageRangeMessage,
    GenericPagesResp, MstConfig, MstKey,
};

type GenericMessageSerde = BincodeMsgSerde<GenericPageRangeMessage>;
type GenericPageQueryType<T> =
    BincodeZrpcType<GenericLoadPageReq, GenericPagesResp<T>, ()>;

/// Implementation of page request handler that directly implements ZrpcServiceHander
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
impl<T, S> ZrpcServiceHander<GenericPageQueryType<T>>
    for MstPageRequestHandlerImpl<T, S>
where
    T: Clone + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
    S: StorageBackend + Send + Sync,
{
    async fn handle(
        &self,
        request: GenericLoadPageReq,
    ) -> Result<GenericPagesResp<T>, ()> {
        let mut items = BTreeMap::new();

        for page in request.pages {
            // Scan storage for keys in the page range
            let entries = match self.storage.scan(&[]).await {
                Ok(entries) => entries,
                Err(err) => {
                    tracing::error!("Failed to scan storage: {}", err);
                    return Ok(GenericPagesResp {
                        items: BTreeMap::new(),
                    });
                }
            };

            for (key_bytes, value_bytes) in entries {
                if key_bytes.len() == 8 {
                    let key_u64 = u64::from_be_bytes(
                        key_bytes.as_slice().try_into().unwrap_or_default(),
                    );

                    if key_u64 >= page.start_bounds
                        && key_u64 <= page.end_bounds
                    {
                        match (self.config.deserialize)(value_bytes.as_slice())
                        {
                            Ok(entry) => {
                                items.insert(key_u64, entry);
                            }
                            Err(err) => {
                                tracing::error!(
                                    "Failed to deserialize entry: {}",
                                    err
                                );
                            }
                        }
                    }
                }
            }
        }

        Ok(GenericPagesResp { items })
    }
}

/// Zenoh-based implementation of MstNetworking with integrated handlers
pub struct ZenohMstNetworking<T, S>
where
    T: Clone
        + Send
        + Sync
        + Serialize
        + for<'de> Deserialize<'de>
        + std::hash::Hash
        + 'static,
    S: StorageBackend + Send + Sync + 'static,
{
    prefix: String,
    zenoh_session: Session,
    server: Arc<
        RwLock<
            ZrpcService<
                MstPageRequestHandlerImpl<T, S>,
                GenericPageQueryType<T>,
            >,
        >,
    >,
    storage: Arc<S>,
    mst: Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
    config: Arc<MstConfig<T>>,
    node_id: u64,
}

impl<T, S> ZenohMstNetworking<T, S>
where
    T: Clone
        + Send
        + Sync
        + Serialize
        + for<'de> Deserialize<'de>
        + std::hash::Hash
        + 'static,
    S: StorageBackend + Send + Sync + 'static,
{
    pub fn new(
        shard_id: u64,
        prefix: String,
        zenoh_session: Session,
        storage: Arc<S>,
        mst: Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
        config: Arc<MstConfig<T>>,
        node_id: u64,
    ) -> Self {
        let handler =
            MstPageRequestHandlerImpl::new(storage.clone(), config.clone());
        let server_config = ServerConfig {
            service_id: format!("{}/page-query/{}", prefix, shard_id),
            ..Default::default()
        };
        let server =
            ZrpcService::new(zenoh_session.clone(), server_config, handler);

        Self {
            prefix,
            zenoh_session,
            server: Arc::new(RwLock::new(server)),
            storage,
            mst,
            config,
            node_id,
        }
    }
}

/// Standalone function to handle page updates
async fn handle_page_update<T, S>(
    storage: &Arc<S>,
    mst: &Arc<RwLock<MerkleSearchTree<MstKey, T>>>,
    config: &Arc<MstConfig<T>>,
    node_id: u64,
    zenoh_session: &Session,
    prefix: &str,
    owner: u64,
    pages: Vec<GenericNetworkPage>,
) -> Result<(), MstError>
where
    T: Clone
        + Send
        + Sync
        + Serialize
        + for<'de> Deserialize<'de>
        + std::hash::Hash
        + 'static,
    S: StorageBackend + Send + Sync,
{
    if owner == node_id {
        return Ok(()); // Skip our own updates
    }

    let remote_pages = GenericNetworkPage::to_page_range(&pages);

    // Compare with local MST to find differences
    let req = {
        let mut mst_guard = mst.write().await;
        let _ = mst_guard.root_hash();
        let local_pages = mst_guard.serialise_page_ranges().unwrap_or(vec![]);
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

    // Create ZRPC client for requesting pages
    let rpc_client: ZrpcClient<GenericPageQueryType<T>> = ZrpcClient::new(
        format!("{}/page-query", prefix),
        zenoh_session.clone(),
    )
    .await;

    // Request missing pages
    let resp = rpc_client
        .call_with_key(owner.to_string(), &req)
        .await
        .map_err(|e| MstError(format!("{:?}", e)))?;

    // Merge received data
    if !resp.items.is_empty() {
        tracing::info!("Merging {} objects from {}", resp.items.len(), owner);

        for (key, remote_value) in resp.items {
            let key_bytes = key.to_be_bytes();

            // Resolve conflicts using merge function
            let final_value = if let Ok(Some(existing_bytes)) =
                storage.get(&key_bytes).await
            {
                let existing = (config.deserialize)(existing_bytes.as_slice())
                    .map_err(|e| MstError(e.to_string()))?;
                (config.merge_function)(existing, remote_value, node_id)
            } else {
                remote_value
            };

            // Store merged result
            let serialized = (config.serialize)(&final_value)
                .map_err(|e| MstError(e.to_string()))?;

            storage
                .put(&key_bytes, StorageValue::from(serialized))
                .await
                .map_err(|e| MstError(e.to_string()))?;

            // Update MST
            let mut mst_guard = mst.write().await;
            mst_guard.upsert(MstKey(key), &final_value);
        }
    }

    Ok(())
}

#[async_trait]
impl<T, S> MstNetworking<T> for ZenohMstNetworking<T, S>
where
    T: Clone
        + Send
        + Sync
        + Serialize
        + for<'de> Deserialize<'de>
        + std::hash::Hash
        + 'static,
    S: StorageBackend + Send + Sync + 'static,
{
    type Error = MstError;

    async fn start(&self) -> Result<(), Self::Error> {
        // Start the ZRPC service
        let mut service = self.server.write().await;
        {
            tracing::debug!("Starting ZRPC service for MST networking");
            service.start().await.map_err(|e| MstError(e.to_string()))?;
        }

        tracing::debug!("Starting subscription for page updates");
        // Start subscription for page updates
        let subscriber = self
            .zenoh_session
            .declare_subscriber(format!("{}/update-pages", self.prefix))
            .await
            .map_err(|e| MstError(e.to_string()))?;

        // Handle page update messages
        let storage = self.storage.clone();
        let mst = self.mst.clone();
        let config = self.config.clone();
        let node_id = self.node_id;
        let zenoh_session = self.zenoh_session.clone();
        let prefix = self.prefix.clone();

        tracing::debug!("Spawning background task for page update handling");
        tokio::spawn(async move {
            tracing::debug!(
                "Page update handler task started for prefix: {}",
                prefix
            );
            loop {
                if let Ok(sample) = subscriber.recv_async().await {
                    // tracing::debug!("Received page update message");
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

                    if let Err(err) = handle_page_update(
                        &storage,
                        &mst,
                        &config,
                        node_id,
                        &zenoh_session,
                        &prefix,
                        msg.owner,
                        msg.pages,
                    )
                    .await
                    {
                        tracing::error!(
                            "Failed to handle page update: {}",
                            err
                        );
                    }
                } else {
                    tracing::debug!(
                        "Page update subscriber closed, exiting loop"
                    );
                    break;
                }
            }
            tracing::debug!(
                "Page update handler task finished for prefix: {}",
                prefix
            );
        });

        tracing::info!(
            "MST networking started successfully for prefix: {}",
            self.prefix
        );
        Ok(())
    }

    async fn stop(&self) -> Result<(), Self::Error> {
        let mut service = self.server.write().await;
        service.close().await;
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
}
