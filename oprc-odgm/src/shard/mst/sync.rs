use std::{collections::BTreeMap, ops::Bound::Included, sync::Arc};

use flare_zrpc::MsgSerde;
use flare_zrpc::{bincode::BincodeZrpcType, ZrpcServiceHander};
use merkle_search_tree::MerkleSearchTree;
use openraft::AnyError;
use tokio::sync::RwLock;

use crate::shard::{
    mst::{
        msg::{NetworkPage, PageRangeMessage},
        MessageSerde,
    },
    ObjectEntry,
};

use super::{
    msg::{Key, LoadPageReq, PagesResp},
    BT,
};

pub type PageQueryType = BincodeZrpcType<LoadPageReq, PagesResp, ()>;
pub struct PageQueryHandler {
    map: Arc<RwLock<BT>>,
}

impl PageQueryHandler {
    pub fn new(map: Arc<RwLock<BT>>) -> Self {
        Self { map }
    }
}

#[async_trait::async_trait]
impl ZrpcServiceHander<PageQueryType> for PageQueryHandler {
    async fn handle(&self, req: LoadPageReq) -> Result<PagesResp, ()> {
        let map = self.map.read().await;
        let mut items = BTreeMap::new();
        for page in req.pages {
            for (k, v) in map
                .range((Included(page.start_bounds), Included(page.end_bounds)))
            {
                items.insert(*k, v.clone());
            }
        }
        Ok(PagesResp { items })
    }
}

pub async fn pub_mst_pages(
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
