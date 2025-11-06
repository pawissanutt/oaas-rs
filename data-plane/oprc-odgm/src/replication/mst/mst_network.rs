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
            let start = page.start_bounds;
            let end = page.end_bounds;
            let entries = match self.storage.scan(&[]).await {
                Ok(e) => e,
                Err(err) => {
                    tracing::error!("Failed to scan storage: {}", err);
                    return Ok(GenericPagesResp {
                        items: BTreeMap::new(),
                    });
                }
            };
            for (key_bytes, value_bytes) in entries {
                // Lexicographic inclusive range
                if key_bytes.as_slice() >= start.as_slice()
                    && key_bytes.as_slice() <= end.as_slice()
                {
                    match (self.config.deserialize)(value_bytes.as_slice()) {
                        Ok(entry) => {
                            items.insert(key_bytes.as_slice().to_vec(), entry);
                        }
                        Err(err) => tracing::error!(
                            "Failed to deserialize entry: {}",
                            err
                        ),
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
    shard_id: u64,
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
            shard_id,
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
    source_shard_id: u64,
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
    // Basic visibility for troubleshooting replication flows
    tracing::info!(
        owner = owner,
        node_id = node_id,
        page_count = pages.len(),
        prefix = %prefix,
        "Received MST page update notification"
    );
    if owner == node_id {
        tracing::debug!("Skipping self-originated page update");
        return Ok(()); // Skip our own updates
    }

    let remote_pages = GenericNetworkPage::to_page_range(&pages);

    // Compare with local MST to find differences
    let mut req = {
        let mut mst_guard = mst.write().await;
        let _ = mst_guard.root_hash();
        let local_pages = mst_guard.serialise_page_ranges().unwrap_or(vec![]);
        let diff_pages = diff(local_pages, remote_pages);
        GenericLoadPageReq::from_diff(diff_pages)
    };

    // Fallback: if diff yields no ranges (e.g., empty local state edge-case),
    // request the published remote pages directly.
    if req.pages.is_empty() {
        req = GenericLoadPageReq::from_pages(&pages);
        if req.pages.is_empty() {
            tracing::debug!("No diff and no direct pages to request; skipping");
            return Ok(());
        }
    }

    tracing::debug!(
        "Requesting {} pages from owner {}",
        req.pages.len(),
        owner
    );

    // Create ZRPC client for requesting pages and retry a few times to avoid transient races
    let rpc_client: ZrpcClient<GenericPageQueryType<T>> = ZrpcClient::new(
        format!("{}/page-query", prefix),
        zenoh_session.clone(),
    )
    .await;

    let mut attempts = 0usize;
    let resp = loop {
        attempts += 1;
        match rpc_client
            .call_with_key(source_shard_id.to_string(), &req)
            .await
        {
            Ok(r) => break r,
            Err(e) if attempts < 3 => {
                tracing::warn!(
                    "Page request to shard {} failed on attempt {}: {:?}; retrying",
                    source_shard_id,
                    attempts,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            Err(e) => return Err(MstError(format!("{:?}", e))),
        }
    };

    // Merge received data
    if !resp.items.is_empty() {
        tracing::info!(
            "Merging {} objects from {} for prefix {}",
            resp.items.len(),
            owner,
            prefix
        );

        for (key_vec, remote_value) in resp.items {
            let key_bytes = key_vec.clone();
            let key_slice = key_bytes.as_slice();
            let final_value = if let Ok(Some(existing_bytes)) =
                storage.get(key_slice).await
            {
                let existing = (config.deserialize)(existing_bytes.as_slice())
                    .map_err(|e| MstError(e.to_string()))?;
                (config.merge_function)(existing, remote_value, node_id)
            } else {
                remote_value
            };
            let serialized = (config.serialize)(&final_value)
                .map_err(|e| MstError(e.to_string()))?;
            storage
                .put(key_slice, StorageValue::from(serialized))
                .await
                .map_err(|e| MstError(e.to_string()))?;
            let mut mst_guard = mst.write().await;
            mst_guard.upsert(MstKey(key_bytes), &final_value);
        }
    } else {
        tracing::info!("No items returned from owner {}", owner);
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
            // Single source: Zenoh subscriber
            loop {
                match subscriber.recv_async().await {
                    Ok(sample) => {
                        let msg = match GenericMessageSerde::from_zbyte(
                            sample.payload(),
                        ) {
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
                            msg.source_shard_id,
                            msg.pages,
                        )
                        .await
                        {
                            tracing::error!(
                                "Failed to handle page update: {}",
                                err
                            );
                        }
                    }
                    Err(_) => {
                        tracing::debug!(
                            "Page update subscriber closed, exiting loop"
                        );
                        break;
                    }
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
        let msg = GenericPageRangeMessage {
            owner,
            source_shard_id: self.shard_id,
            pages,
        };
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
