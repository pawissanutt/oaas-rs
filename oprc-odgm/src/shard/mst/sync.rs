use std::{collections::BTreeMap, ops::Bound::Included, sync::Arc};

use flare_zrpc::{bincode::BincodeZrpcType, ZrpcServiceHander};
use tokio::sync::RwLock;

use super::{
    msg::{LoadPageReq, PagesResp},
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
