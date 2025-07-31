use crate::error::OdgmError;
use crate::shard::mst::sync::pub_mst_pages;
use crate::shard::{ShardMetadata, ShardState};

use super::{Key, LoadPageReq, NetworkPage, PageRangeMessage, PagesResp};
use super::{ObjectEntry, PageQueryType};
use flare_zrpc::ZrpcServiceHander;
use flare_zrpc::{
    server::{ServerConfig, ZrpcService},
    MsgSerde, ZrpcClient,
};
use merkle_search_tree::{diff::diff, MerkleSearchTree};
use openraft::AnyError;
use rand::Rng;
use std::{collections::BTreeMap, error::Error, sync::Arc};
use tokio::sync::{
    watch::{Receiver, Sender},
    Mutex, RwLock,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use zenoh::sample::Sample;

use crossbeam_skiplist::SkipMap;

type DHM = SkipMap<u64, ObjectEntry>;
type MST = MerkleSearchTree<Key, ObjectEntry>;

pub struct ObjectMstShard {
    shard_metadata: ShardMetadata,
    map: Arc<DHM>,
    mst: Arc<RwLock<MST>>,
    z_session: zenoh::Session,
    token: CancellationToken,
    prefix: String,
    server: Mutex<ZrpcService<PageQueryHandler, PageQueryType>>,
    readiness_sender: Sender<bool>,
    readiness_receiver: Receiver<bool>,
}

impl ObjectMstShard {
    pub fn new(
        z_session: zenoh::Session,
        shard_metadata: ShardMetadata,
        rpc_prefix: String,
    ) -> Self {
        let map = Arc::new(DHM::new());
        let mst = Arc::new(RwLock::new(MerkleSearchTree::default()));
        let handler = PageQueryHandler::new(map.clone());
        let conf = ServerConfig {
            service_id: format!(
                "{}/page-query/{}",
                rpc_prefix, shard_metadata.id
            ),
            ..Default::default()
        };
        let server = ZrpcService::new(z_session.clone(), conf, handler);
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            shard_metadata,
            map,
            mst,
            z_session,
            token: CancellationToken::new(),
            prefix: rpc_prefix,
            server: Mutex::new(server),
            readiness_sender: tx,
            readiness_receiver: rx,
        }
    }

    async fn start_sub_loop(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let id = self.shard_metadata.id;
        let mst = self.mst.clone();
        let map = self.map.clone();
        let (tx, rx) = flume::unbounded();
        self.z_session
            .declare_subscriber(format!("{}/update-pages", self.prefix))
            .callback(move |query| tx.send(query).unwrap())
            .background()
            .await?;
        for _ in 0..2 {
            let local_token = self.token.clone();
            let local_rx = rx.clone();
            let local_mst = mst.clone();
            let local_map = map.clone();
            let client = ZrpcClient::new(
                format!("{}/page-query", self.prefix),
                self.z_session.clone(),
            )
            .await;
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = local_token.cancelled() => {break;},
                        receive = local_rx.recv_async() => {
                            match receive {
                                Ok(sample) => {
                                    handle_sample(id, &local_mst, &local_map, &client, sample).await;
                                },
                                Err(err) => {
                                    tracing::error!("shard '{}': sub:error: {}", id, err,);
                                    break;
                                },
                            }
                        }
                    }
                }
            });
        }
        Ok(())
    }

    fn start_pub_loop(&self) {
        let z = self.z_session.clone();
        let id = self.shard_metadata.id;
        let prefix = self.prefix.clone();
        let interval: u64 = self
            .shard_metadata
            .options
            .get("mst_sync_interval")
            .unwrap_or(&"5000".to_string())
            .parse()
            .expect("Failed to parse sync interval");
        let token = self.token.clone();
        let mst = self.mst.clone();

        info!(
            "shard '{}': starting pub loop with interval {} ms",
            id, interval
        );
        tokio::spawn(async move {
            let publisher = z
                .declare_publisher(format!("{}/update-pages", prefix))
                .allowed_destination(zenoh::sample::Locality::Remote)
                .await
                .expect("Failed to declare publisher");
            let random = {
                let mut rng = rand::rng();
                rng.random_range(0..interval)
            };
            tokio::time::sleep(std::time::Duration::from_millis(random)).await;
            loop {
                if let Err(err) = pub_mst_pages(id, &mst, &publisher).await {
                    tracing::error!(
                        "Failed to publish page range message: {}",
                        err
                    );
                    continue;
                }
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(interval)) => {}
                }
            }
        });
    }

    #[allow(dead_code)]
    pub async fn trigger_pub_mst_pages(&self) -> Result<(), AnyError> {
        let mut mst_gaurd = self.mst.write().await;
        mst_gaurd.root_hash();
        if let Some(pages) = mst_gaurd.serialise_page_ranges() {
            let pages = NetworkPage::from_page_ranges(pages);
            drop(mst_gaurd);
            let page_len = pages.len();
            let id = self.meta().id;
            let msg = PageRangeMessage { owner: id, pages };
            let payload = super::MessageSerde::to_zbyte(&msg)?;
            tracing::debug!(
                "shard {}: sending page range with {} pages",
                self.meta().id,
                page_len
            );
            if let Err(err) = self
                .z_session
                .put(format!("{}/update-pages", self.prefix), payload)
                .await
            {
                tracing::error!(
                    "Failed to publish page range message: {}",
                    err
                );
            }
        }
        Ok(())
    }
}

async fn handle_sample(
    id: u64,
    mst: &Arc<RwLock<MST>>,
    map: &Arc<DHM>,
    client: &ZrpcClient<PageQueryType>,
    sample: Sample,
) {
    let msg = match super::MessageSerde::from_zbyte(sample.payload()) {
        Ok(msg) => msg,
        Err(err) => {
            tracing::error!(
                "shard '{}':Failed to decode page range message: {}",
                id,
                err
            );
            return;
        }
    };
    if msg.owner == id {
        return;
    }
    let owner = msg.owner;
    let remote_pages = NetworkPage::to_page_range(&msg.pages);
    let req = {
        let mut mst_2 = mst.write().await;
        let _ = mst_2.root_hash();
        let local_pages = mst_2.serialise_page_ranges().unwrap_or(vec![]);
        debug!("shard '{}': local pages: {:?}", id, local_pages);
        debug!("shard '{}': remote pages: {:?}", id, remote_pages);
        let diff_pages = diff(local_pages, remote_pages);
        debug!("shard '{}': diff pages: {:?}", id, diff_pages);
        let req = LoadPageReq::from_diff(diff_pages);
        req
    };

    tracing::debug!(
        "shard '{}': receive update-pages with {} pages, found diff {} pages",
        id,
        msg.pages.len(),
        req.pages.len()
    );

    if req.pages.len() == 0 {
        return;
    }

    tracing::debug!(
        "shard '{}': sending load page request ({} pages) to owner {}",
        id,
        req.pages.len(),
        owner
    );
    let resp = match client.call_with_key(owner.to_string(), &req).await {
        Ok(resp) => resp,
        Err(err) => {
            tracing::error!(
                "shard '{}':Failed to send load page request: {:?}",
                id,
                err
            );
            return;
        }
    };
    tracing::info!("merging {} objects from {}", resp.items.len(), owner);
    merge_obj(id, map, mst, resp).await;
}

async fn merge_obj(
    id: u64,
    map: &Arc<DHM>,
    mst: &Arc<RwLock<MST>>,
    resp: PagesResp,
) {
    if resp.items.is_empty() {
        return;
    }
    let mut mst = mst.write().await;
    for (k, mut v) in resp.items.into_iter() {
        match map.get(&k) {
            Some(entry) => {
                let old = entry.value();
                match v.merge_cloned(old) {
                    Ok(_) => {
                        tracing::debug!(
                            "shard {}: successfully merged object {}",
                            id,
                            k
                        );
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to merge object {}: {}",
                            k,
                            err
                        );
                        continue;
                    }
                }

                mst.upsert(Key(k), &v);
                map.insert(k, v);
            }
            None => {
                tracing::debug!(
                    "shard {}: inserting new object with key {}",
                    id,
                    k
                );
                mst.upsert(Key(k), &v);
                map.insert(k, v);
            }
        };
    }
    // tracing::debug!("finished merging objects");
}

#[async_trait::async_trait]
impl ShardState for ObjectMstShard {
    type Key = u64;
    type Entry = ObjectEntry;

    #[inline]
    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn initialize(&self) -> Result<(), OdgmError> {
        self.start_sub_loop().await?;
        self.start_pub_loop();
        self.server.lock().await.start().await?;
        self.readiness_sender.send(true).unwrap();
        Ok(())
    }

    async fn close(&mut self) -> Result<(), OdgmError> {
        self.server.lock().await.close().await;
        self.token.cancel();
        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, OdgmError> {
        let out = self.map.get(key);
        let out = out.map(|r| r.value().clone());
        Ok(out)
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), OdgmError> {
        let mut mst = self.mst.write().await;
        mst.upsert(Key(key), &value);
        drop(mst);
        self.map.insert(key, value);
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError> {
        self.map.remove(key);
        Ok(())
    }

    #[inline]
    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }

    #[inline]
    async fn count(&self) -> Result<u64, OdgmError> {
        Ok(self.map.len() as u64)
    }
}

pub struct PageQueryHandler {
    map: Arc<DHM>,
}

impl PageQueryHandler {
    pub fn new(map: Arc<DHM>) -> Self {
        Self { map }
    }
}

use std::ops::Bound::Included;

#[async_trait::async_trait]
impl ZrpcServiceHander<PageQueryType> for PageQueryHandler {
    async fn handle(&self, req: LoadPageReq) -> Result<PagesResp, ()> {
        let mut items = BTreeMap::new();
        for page in req.pages {
            for entry in self
                .map
                .range((Included(page.start_bounds), Included(page.end_bounds)))
            {
                items.insert(*entry.key(), entry.value().clone());
            }
        }
        Ok(PagesResp { items })
    }
}
