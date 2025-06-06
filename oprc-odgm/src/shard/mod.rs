mod basic;
pub mod factory;
mod invocation;
mod liveliness;
pub mod manager;
mod mst;
mod network;
// mod proxy;
mod raft;

use std::collections::HashMap;
use std::sync::Arc;

use automerge::AutomergeError;
pub use basic::BasicObjectShard;
pub use basic::ObjectEntry;
pub use basic::ObjectVal;
use invocation::InvocationNetworkManager;
use invocation::InvocationOffloader;
use liveliness::MemberLivelinessState;
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
    ) -> Result<Option<Self::Entry>, OdgmError>;

    async fn merge(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<Self::Entry, OdgmError> {
        self.set(key.to_owned(), value).await?;
        let item = self.get(&key).await?;
        match item {
            Some(entry) => Ok(entry),
            None => Err(OdgmError::InvalidArgument(
                "Merged result is None".to_string(),
            )),
        }
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), OdgmError>;

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError>;

    async fn count(&self) -> Result<u64, OdgmError>;
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
    z_session: zenoh::Session,
    pub(crate) shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>,
    pub(crate) inv_net_manager: Arc<Mutex<InvocationNetworkManager>>,
    pub(crate) inv_offloader: Arc<InvocationOffloader>,
    network: Arc<Mutex<ShardNetwork>>,
    token: CancellationToken,
    liveliness_state: MemberLivelinessState,
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

        let inv_offloader = Arc::new(InvocationOffloader::new(&shard_metadata));
        let inv_net_manager = InvocationNetworkManager::new(
            z_session.clone(),
            shard_metadata.clone(),
            inv_offloader.clone(),
        );
        let network =
            ShardNetwork::new(z_session.clone(), shard_state.clone(), prefix);
        Self {
            z_session,
            shard_state: shard_state.clone(),
            network: Arc::new(Mutex::new(network)),
            inv_net_manager: Arc::new(Mutex::new(inv_net_manager)),
            inv_offloader,
            token: CancellationToken::new(),
            liveliness_state: MemberLivelinessState::default(),
        }
    }

    async fn initialize(&self) -> Result<(), OdgmError> {
        self.shard_state.initialize().await?;
        self.inv_net_manager
            .lock()
            .await
            .start()
            .await
            .map_err(|e| OdgmError::ZenohError(e))?;
        self.sync_network();
        self.liveliness_state
            .declare_liveliness(&self.z_session, self.meta())
            .await;
        self.liveliness_state
            .update(&self.z_session, self.meta())
            .await;
        Ok(())
    }

    fn sync_network(&self) {
        let shard = self.clone();
        let session = self.z_session.clone();
        tokio::spawn(async move {
            let mut receiver = shard.shard_state.watch_readiness();
            let liveliness_selector = format!(
                "oprc/{}/{}/liveliness/*",
                shard.shard_state.meta().collection,
                shard.shard_state.meta().partition_id
            );
            let liveliness_sub = match session
                .liveliness()
                .declare_subscriber(liveliness_selector)
                .await
            {
                Ok(sub) => sub,
                Err(e) => {
                    error!(
                        "shard {}: Failed to declare liveliness subscriber: {}",
                        shard.shard_state.meta().id,
                        e
                    );
                    return;
                }
            };
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
                    token = liveliness_sub.recv_async() => {
                        match token {
                            Ok(sample) => {
                                let id = shard.liveliness_state.handle_sample(&sample).await;
                                info!("shard {}: liveliness {id:?} updated {sample:?}", shard.shard_state.meta().id );
                                if id != Some(shard.shard_state.meta().id) {
                                    let mut inv = shard.inv_net_manager.lock().await;
                                    inv.on_liveliness_updated(&shard.liveliness_state).await;
                                }
                            },
                            Err(_) => {},
                        };
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
        self.inv_net_manager.lock().await.stop().await;
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
