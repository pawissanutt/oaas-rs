use std::sync::Arc;

use flare_zrpc::bincode::BincodeZrpcType;
use flare_zrpc::MsgSerde;
use openraft::AnyError;
use tokio::sync::RwLock;

use crate::shard::mst::{
    msg::{NetworkPage, PageRangeMessage},
    MessageSerde,
};

use super::msg::{LoadPageReq, PagesResp};

pub type PageQueryType = BincodeZrpcType<LoadPageReq, PagesResp, ()>;

use super::MST;

pub async fn pub_mst_pages(
    id: u64,
    mst: &Arc<RwLock<MST>>,
    publisher: &zenoh::pubsub::Publisher<'_>,
) -> Result<(), AnyError> {
    let mut mst_guard = mst.write().await;
    mst_guard.root_hash();
    if let Some(pages) = mst_guard.serialise_page_ranges() {
        let pages = NetworkPage::from_page_ranges(pages);
        drop(mst_guard);
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
