mod msg;
mod sync;

use super::{ObjectEntry, ShardState};
use flare_dht::{error::FlareError, shard::ShardMetadata};
use flare_zrpc::{
    server::{ServerConfig, ZrpcService},
    MsgSerde, ZrpcClient,
};
use merkle_search_tree::{diff::diff, MerkleSearchTree};
use msg::{Key, LoadPageReq, NetworkPage, PageRangeMessage, PagesResp};
use openraft::AnyError;
use rand::Rng;
use std::{collections::BTreeMap, error::Error, sync::Arc};
use tokio::sync::{
    watch::{Receiver, Sender},
    RwLock,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};
use zenoh::sample::Sample;

type BT = BTreeMap<u64, ObjectEntry>;
type MST = MerkleSearchTree<Key, ObjectEntry>;

pub struct ObjectMstShard {
    shard_metadata: ShardMetadata,
    map: Arc<RwLock<BT>>,
    mst: Arc<RwLock<MST>>,
    z_session: zenoh::Session,
    token: CancellationToken,
    prefix: String,
    server: ZrpcService<sync::PageQueryHandler, sync::PageQueryType>,
    readiness_sender: Sender<bool>,
    readiness_receiver: Receiver<bool>,
    // sender: UnboundedSender<ObjectChangedEvent>,
}

#[allow(dead_code)]
type MessageSerde = flare_zrpc::bincode::BincodeMsgSerde<PageRangeMessage>;

impl ObjectMstShard {
    pub fn new(
        z_session: zenoh::Session,
        shard_metadata: ShardMetadata,
        rpc_prefix: String,
    ) -> Self {
        let map = Arc::new(RwLock::new(BTreeMap::new()));
        let handler = sync::PageQueryHandler::new(map.clone());
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
            mst: Arc::new(RwLock::new(MerkleSearchTree::default())),
            z_session,
            token: CancellationToken::new(),
            prefix: rpc_prefix,
            server,
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
            let local_map: Arc<RwLock<BTreeMap<u64, ObjectEntry>>> =
                map.clone();
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
            .get("sync_interval")
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
                let mut rng = rand::thread_rng();
                rng.gen_range(0..interval)
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
            let payload = MessageSerde::to_zbyte(&msg)?;
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
    map: &Arc<RwLock<BT>>,
    client: &ZrpcClient<sync::PageQueryType>,
    sample: Sample,
) {
    let msg = match MessageSerde::from_zbyte(sample.payload()) {
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
    let mut mst_2 = mst.write().await;
    let _ = mst_2.root_hash();
    let local_pages = mst_2.serialise_page_ranges().unwrap_or(vec![]);
    debug!("shard '{}': local pages: {:?}", id, local_pages);
    debug!("shard '{}': remote pages: {:?}", id, remote_pages);
    let diff_pages = diff(local_pages, remote_pages);
    debug!("shard '{}': diff pages: {:?}", id, diff_pages);
    let req = LoadPageReq::from_diff(diff_pages);

    tracing::debug!(
        "shard '{}': receive update-pages with {} pages, found diff {} pages",
        id,
        msg.pages.len(),
        req.pages.len()
    );
    drop(mst_2);
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
    map: &Arc<RwLock<BT>>,
    mst: &Arc<RwLock<MST>>,
    resp: PagesResp,
) {
    if resp.items.is_empty() {
        return;
    }
    let mut map = map.write().await;
    let mut mst = mst.write().await;
    for (k, v) in resp.items.into_iter() {
        match map.get_mut(&k) {
            Some(old) => {
                match old.merge(v) {
                    Ok(merged) => {
                        tracing::debug!(
                            "shard {}: successfully merged object {}",
                            id,
                            k
                        );
                        merged
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
                mst.upsert(Key(k), &old);
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

async fn pub_mst_pages(
    id: u64,
    mst: &Arc<RwLock<MerkleSearchTree<Key, ObjectEntry>>>,
    publisher: &zenoh::pubsub::Publisher<'_>,
) -> Result<(), AnyError> {
    let mut mst_gaurd = mst.write().await;
    mst_gaurd.root_hash();
    if let Some(pages) = mst_gaurd.serialise_page_ranges() {
        let pages = NetworkPage::from_page_ranges(pages);
        drop(mst_gaurd);
        let page_len = pages.len();
        let msg = PageRangeMessage { owner: id, pages };
        let payload = MessageSerde::to_zbyte(&msg)?;
        tracing::debug!(
            "shard {}: sending page range with {} pages",
            id,
            page_len
        );
        if let Err(err) = publisher.put(payload).await {
            tracing::error!("Failed to publish page range message: {}", err);
        }
    }
    Ok(())
}

#[async_trait::async_trait]
impl ShardState for ObjectMstShard {
    type Key = u64;
    type Entry = ObjectEntry;

    #[inline]
    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn initialize(&self) -> Result<(), FlareError> {
        self.start_sub_loop().await?;
        self.start_pub_loop();
        self.server.start().await?;
        self.readiness_sender.send(true).unwrap();
        Ok(())
    }

    async fn close(&self) -> Result<(), FlareError> {
        self.server.close();
        self.token.cancel();
        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, FlareError> {
        let map = self.map.read().await;

        let out = map.get(key);
        let out = out.map(|r| r.clone());
        Ok(out)
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), FlareError> {
        let mut mst = self.mst.write().await;
        mst.upsert(Key(key), &value);
        drop(mst);
        let mut map = self.map.write().await;
        map.insert(key, value);
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError> {
        let mut map = self.map.write().await;
        map.remove(key);
        Ok(())
    }

    #[inline]
    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }
}

#[cfg(test)]
mod test {

    use crate::shard::{mst::ObjectMstShard, ObjectEntry, ShardState};
    use flare_dht::shard::ShardMetadata;

    async fn create_shard(meta: ShardMetadata) -> ObjectMstShard {
        let z_session =
            zenoh::open(zenoh::config::Config::default()).await.unwrap();
        let col = meta.collection.clone();
        let partition_id = meta.partition_id;
        let shard = ObjectMstShard::new(
            z_session,
            meta,
            format!("oprc/{}/{}", col, partition_id),
        );
        shard.initialize().await.unwrap();
        shard
    }

    #[tracing_test::traced_test]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_single() {
        tracing::info!("start...");
        let shard_metadata = ShardMetadata {
            id: 1,
            collection: "test".to_string(),
            partition_id: 1,
            owner: Some(1),
            primary: Some(1),
            replica_owner: vec![1],
            replica: vec![1],
            ..Default::default()
        };
        tracing::debug!("creating...");
        let shard = create_shard(shard_metadata).await;
        let obj = ObjectEntry::random(10);
        tracing::debug!("setting...");
        shard.set(1, obj.clone()).await.unwrap();
        tracing::debug!("getting...");
        let v = shard.get(&1).await.unwrap();
        assert_eq!(v, Some(obj));
        tracing::debug!("deleting...");
        shard.delete(&1).await.unwrap();
        tracing::debug!("getting...");
        let v = shard.get(&1).await.unwrap();
        assert_eq!(v, None);
        shard.close().await.unwrap();
    }

    #[tracing_test::traced_test]
    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_multiple() {
        let shard_1 = create_shard(ShardMetadata {
            id: 1,
            collection: "test".to_string(),
            partition_id: 0,
            owner: Some(1),
            replica_owner: vec![1, 2],
            replica: vec![1, 2],
            ..Default::default()
        })
        .await;

        let shard_2 = create_shard(ShardMetadata {
            id: 2,
            collection: "test".to_string(),
            partition_id: 0,
            owner: Some(2),
            replica_owner: vec![1, 2],
            replica: vec![1, 2],
            ..Default::default()
        })
        .await;
        for i in 0..20 {
            shard_1.set(i, ObjectEntry::random(1)).await.unwrap();
        }
        // {
        //     shard_1.mst.write().await.root_hash();
        // }
        // debug!("shard 1 : tree {:?}", shard_1.mst.read().await);

        for i in 20..40 {
            shard_2.set(i, ObjectEntry::random(1)).await.unwrap();
        }
        // {
        //     shard_2.mst.write().await.root_hash();
        // }
        // debug!("shard 2 : tree {:?}", shard_2.mst.read().await);

        shard_1.trigger_pub_mst_pages().await.unwrap();
        shard_2.trigger_pub_mst_pages().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        shard_2.trigger_pub_mst_pages().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        shard_1.trigger_pub_mst_pages().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        for i in 0..20 {
            let v = shard_2.get(&i).await.unwrap();
            assert_ne!(v, None, "key {} not found", i);
        }

        for i in 20..40 {
            let v = shard_1.get(&i).await.unwrap();
            assert_ne!(v, None, "key {} not found", i);
        }

        shard_1.close().await.unwrap();
        shard_2.close().await.unwrap();
    }
}
