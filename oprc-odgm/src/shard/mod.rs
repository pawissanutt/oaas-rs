mod basic;
pub mod factory;
mod invocation;
pub mod manager;
pub(crate) mod msg;
mod mst;
mod network;
mod raft;

use std::collections::HashMap;
use std::sync::Arc;

use automerge::AutomergeError;
pub use basic::BasicObjectShard;
pub use basic::ObjectEntry;
pub use basic::ObjectVal;
use flare_dht::error::FlareError;
use invocation::InvocationOffloader;
use mst::ObjectMstShard;
use network::ShardNetwork;
use oprc_pb::InvocationRoute;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::info;

use crate::error::OdgmError;

#[derive(thiserror::Error, Debug)]
pub enum ShardError {
    #[error("Merge Error: {0}")]
    MergeError(#[from] AutomergeError),
}

#[async_trait::async_trait]
pub trait ShardState: Send + Sync {
    type Key: Send + Clone;
    type Entry: Send + Sync + Default;

    fn meta(&self) -> &ShardMetadata;

    async fn initialize(&self) -> Result<(), OdgmError> {
        Ok(())
    }

    async fn close(&mut self) -> Result<(), OdgmError> {
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool>;

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, FlareError>;

    // async fn modify<F, O>(
    //     &self,
    //     key: &Self::Key,
    //     f: F,
    // ) -> Result<O, FlareError>
    // where
    //     F: FnOnce(&mut Self::Entry) -> O + Send;

    async fn merge(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<Self::Entry, FlareError> {
        self.set(key.to_owned(), value).await?;
        let item = self.get(&key).await?;
        match item {
            Some(entry) => Ok(entry),
            None => Err(FlareError::InvalidArgument(
                "Merged result is None".to_string(),
            )),
        }
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), FlareError>;

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError>;
}

#[async_trait::async_trait]
pub trait ShardFactory: Send + Sync {
    type Key;
    type Entry;
    async fn create_shard(
        &self,
        shard_metadata: ShardMetadata,
    ) -> Result<ObjectShard, OdgmError>;
}

pub type ObjectShardState = Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>;

#[derive(Clone)]
pub struct ObjectShard {
    shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>,
    invocation_offloader: Arc<Mutex<InvocationOffloader>>,
    network: Arc<Mutex<ShardNetwork>>,
    token: CancellationToken,
}

impl ObjectShard {
    fn new(
        shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>,
        z_session: zenoh::Session,
    ) -> Self {
        let shard_metadata = shard_state.meta();
        let prefix = format!(
            "oprc/{}/{}/objects",
            shard_metadata.collection.clone(),
            shard_metadata.partition_id,
        );

        let invocation_offloader =
            InvocationOffloader::new(z_session.clone(), shard_metadata.clone());
        let network = ShardNetwork::new(z_session, shard_state.clone(), prefix);
        Self {
            shard_state: shard_state.clone(),
            network: Arc::new(Mutex::new(network)),
            invocation_offloader: Arc::new(Mutex::new(invocation_offloader)),
            token: CancellationToken::new(),
        }
    }

    async fn initialize(&self) -> Result<(), OdgmError> {
        self.shard_state.initialize().await?;
        self.invocation_offloader
            .lock()
            .await
            .start()
            .await
            .map_err(|e| OdgmError::ZenohError(e))?;
        self.sync_network();
        Ok(())
    }

    fn sync_network(&self) {
        let shard = self.clone();
        tokio::spawn(async move {
            let mut receiver = shard.shard_state.watch_readiness();
            loop {
                tokio::select! {
                    res = receiver.changed() => {
                        if let Err(e) = res {
                            error!("Failed to receive readiness change: {}", e);
                            break;
                        }
                        let mut network = shard.network.lock().await;
                        if receiver.borrow().to_owned() {
                            if !network.is_running() {
                                info!("Start network for shard {}", shard.shard_state.meta().id);
                                if let Err(e) = network.start().await {
                                    error!("Failed to start network: {}", e);
                                }
                            }
                        } else {
                            if network.is_running() {
                                network.stop().await;
                            }
                        }
                    }
                    _ = shard.token.cancelled() => {
                        break;
                    }
                }
            }
            info!(
                "shard {}: Network sync loop exited",
                shard.shard_state.meta().id
            );
        });
    }

    async fn close(self) -> Result<(), OdgmError> {
        info!("{:?}: closing", self.shard_state.meta());
        self.token.cancel();
        let mut network = self.network.lock().await;
        if network.is_running() {
            network.stop().await;
        }
        self.invocation_offloader.lock().await.stop().await;
        Ok(())
    }
}

impl std::ops::Deref for ObjectShard {
    type Target = ObjectShardState;

    fn deref(&self) -> &Self::Target {
        &self.shard_state
    }
}

#[derive(Debug, Default, Clone)]
pub struct ShardMetadata {
    pub id: u64,
    pub collection: String,
    pub partition_id: u16,
    pub owner: Option<u64>,
    pub primary: Option<u64>,
    pub replica: Vec<u64>,
    pub replica_owner: Vec<u64>,
    pub shard_type: String,
    pub options: HashMap<String, String>,
    pub invocations: InvocationRoute,
}
