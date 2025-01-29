use flare_dht::error::FlareError;

use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::{debug, info};

use crate::{
    metadata::OprcMetaManager,
    shard::{manager::ShardManager, ObjectShard},
};

type ShardManader = ShardManager;

pub struct ObjectDataGridManager {
    pub metadata_manager: Arc<OprcMetaManager>,
    pub addr: String,
    pub node_id: u64,
    pub shard_manager: Arc<ShardManader>,
    close_signal_sender: tokio::sync::watch::Sender<bool>,
    close_signal_receiver: tokio::sync::watch::Receiver<bool>,
}

impl ObjectDataGridManager {
    pub async fn new(
        addr: String,
        node_id: u64,
        metadata_manager: Arc<OprcMetaManager>,
        shard_manager: Arc<ShardManader>,
    ) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            metadata_manager: metadata_manager,
            addr,
            node_id,
            shard_manager: shard_manager,
            close_signal_sender: tx,
            close_signal_receiver: rx,
        }
    }

    pub fn start_watch_stream(self: Arc<Self>) {
        let mut rs = tokio_stream::wrappers::WatchStream::new(
            self.metadata_manager.create_watch(),
        );
        info!("start_watch_stream");
        tokio::spawn(async move {
            let mut last_sync = 0;
            loop {
                if let Some(log_id) = rs.next().await {
                    debug!("next {log_id} > {last_sync}");
                    if log_id > last_sync {
                        last_sync = log_id;
                        let local_shards =
                            self.metadata_manager.local_shards().await;
                        debug!("sync_shards {:?}", local_shards);
                        self.shard_manager.sync_shards(&local_shards).await;
                    }
                    if self.close_signal_receiver.has_changed().unwrap_or(true)
                    {
                        info!("closed watch loop");
                        break;
                    }
                }
            }
        });
    }

    // pub async fn join(&self, peer_addr: &str) -> Result<(), Box<dyn Error>> {
    //     info!("advertise addr {}", self.addr);
    //     let peer_addr: Uri = Uri::from_str(peer_addr)?;
    //     let channel = Channel::builder(peer_addr).connect_lazy();
    //     let mut client = FlareControlClient::new(channel);
    //     let resp = client
    //         .join(JoinRequest {
    //             node_id: self.node_id,
    //             addr: self.addr.clone(),
    //         })
    //         .await?;
    //     resp.into_inner();
    //     Ok(())
    // }

    pub async fn close(&self) {
        self.close_signal_sender.send(true).unwrap();
        self.shard_manager.close().await;
    }

    pub async fn get_shard(
        &self,
        collection: &str,
        key: &[u8],
    ) -> Result<ObjectShard, FlareError> {
        let option = self.metadata_manager.get_shard_id(collection, key).await;
        if let Some(group) = option {
            self.shard_manager.get_any_shard(&group.shard_ids)
        } else {
            Err(FlareError::NoCollection(collection.into()))
        }
    }
}
