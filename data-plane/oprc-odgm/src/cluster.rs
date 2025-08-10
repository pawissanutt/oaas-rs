use tokio_util::sync::CancellationToken;

use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::{debug, info};

use crate::{
    metadata::OprcMetaManager,
    shard::{ArcUnifiedObjectShard, UnifiedShardManager},
};

pub struct ObjectDataGridManager {
    pub metadata_manager: Arc<OprcMetaManager>,
    pub node_id: u64,
    pub shard_manager: Arc<UnifiedShardManager>,
    token: CancellationToken,
}

impl ObjectDataGridManager {
    pub async fn new(
        node_id: u64,
        metadata_manager: Arc<OprcMetaManager>,
        shard_manager: Arc<UnifiedShardManager>,
    ) -> Self {
        Self {
            metadata_manager: metadata_manager,
            node_id,
            shard_manager: shard_manager,
            token: CancellationToken::new(),
        }
    }

    pub fn start_watch_stream(&self) {
        let mut rs = tokio_stream::wrappers::WatchStream::new(
            self.metadata_manager.create_watch(),
        );
        info!("start_watch_stream");
        let mm = self.metadata_manager.clone();
        let sm = self.shard_manager.clone();
        let token = self.token.clone();
        tokio::spawn(async move {
            let mut last_sync = 0;
            loop {
                tokio::select! {
                    log = rs.next() => {
                        if let Some(log_id) = log {
                            debug!("next {log_id} > {last_sync}");
                            if log_id > last_sync {
                                last_sync = log_id;
                                let local_shards = mm.local_shards.read().await;
                                debug!("sync_shards {:?}", local_shards);
                                for shard_meta in local_shards.values() {
                                    sm.sync_shards(shard_meta).await;
                                }
                            }
                        }
                    }
                    _ = token.cancelled() => {
                        info!("cancelled watch loop");
                        break;
                    }
                }
            }
        });
    }

    pub async fn close(&self) {
        info!("closing");
        self.token.cancel();
        self.shard_manager.close().await;
    }

    pub async fn get_local_shard(
        &self,
        collection: &str,
        pid: u16,
    ) -> Option<ArcUnifiedObjectShard> {
        let option = self.metadata_manager.get_shard_ids(collection).await;
        if let Some(groups) = option {
            if let Some(group) = groups.get(pid as usize) {
                for s in &group.shard_ids {
                    if let Some(shard) = self.shard_manager.get_shard(*s) {
                        return Some(shard);
                    }
                }
            }
        }
        None
    }

    pub async fn get_any_local_shard(
        &self,
        collection: &str,
    ) -> Option<ArcUnifiedObjectShard> {
        let option = self.metadata_manager.get_shard_ids(collection).await;
        debug!("get_any_local_shard {:?}", option);
        if let Some(groups) = option {
            for group in groups.iter() {
                for s in &group.shard_ids {
                    if let Some(shard) = self.shard_manager.get_shard(*s) {
                        return Some(shard);
                    }
                }
            }
        }
        None
    }
}
