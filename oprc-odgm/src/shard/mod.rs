mod basic;
mod event;
pub mod manager;
pub(crate) mod msg;
mod network;
// mod proxy;
mod invocation;
mod raft;
mod weak;

use std::sync::Arc;

use automerge::AutomergeError;
pub use basic::ObjectEntry;
pub use basic::ObjectShard;
pub use basic::ObjectVal;
use flare_dht::error::FlareError;
use flare_dht::shard::ShardMetadata;
use network::ShardNetwork;
use oprc_zenoh::OprcZenohConfig;
use scc::HashMap;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::error;
use tracing::info;
use weak::ObjectMstShard;
use zenoh::session;

#[derive(thiserror::Error, Debug)]
pub enum ShardError {
    #[error("Merge Error: {0}")]
    MergeError(#[from] AutomergeError),
}

pub struct UnifyShardFactory {
    conf: OprcZenohConfig,
}

impl UnifyShardFactory {
    pub fn new(conf: OprcZenohConfig) -> Self {
        Self { conf }
    }

    async fn create_raft(
        &self,
        z_session: zenoh::Session,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Shard {
        info!("create raft shard {:?}", &shard_metadata);
        let rpc_prefix = format!(
            "oprc/{}/{}",
            shard_metadata.collection, shard_metadata.partition_id
        );
        let shard = raft::RaftObjectShard::new(
            z_session.clone(),
            rpc_prefix,
            shard_metadata,
        )
        .await;
        Shard::new(Arc::new(shard), z_session)
    }

    async fn create_weak(
        &self,
        z_session: zenoh::Session,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Shard {
        info!("create weak shard {:?}", &shard_metadata);
        let shard = ObjectMstShard::new(z_session.clone(), shard_metadata);
        Shard::new(Arc::new(shard), z_session)
    }
}

#[async_trait::async_trait]
impl ShardFactory for UnifyShardFactory {
    type Key = u64;

    type Entry = ObjectEntry;
    async fn create_shard(
        &self,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Shard {
        tracing::info!("create shard {:?}", &shard_metadata);
        let z_session = session::open(self.conf.create_zenoh()).await.unwrap();
        if shard_metadata.shard_type.eq("raft") {
            self.create_raft(z_session, shard_metadata).await
        } else if shard_metadata.shard_type.eq("weak") {
            self.create_weak(z_session, shard_metadata).await
        } else {
            let shard = ObjectShard {
                shard_metadata: shard_metadata,
                map: HashMap::new(),
            };
            Shard::new(Arc::new(shard), z_session)
        }
    }
}

#[async_trait::async_trait]
pub trait ShardState: Send + Sync {
    type Key: Send + Clone;
    type Entry: Send + Sync + Default;

    fn meta(&self) -> &ShardMetadata;

    async fn initialize(&self) -> Result<(), FlareError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), FlareError> {
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        let (_, rx) = tokio::sync::watch::channel(true);
        rx
    }

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
    async fn create_shard(&self, shard_metadata: ShardMetadata) -> Shard;
}

pub type ObjectShardState = Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>;

#[derive(Clone)]
pub struct Shard {
    shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>,
    network: Arc<Mutex<ShardNetwork>>,
    token: CancellationToken,
}

impl Shard {
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

        Self {
            shard_state: shard_state.clone(),
            network: Arc::new(Mutex::new(ShardNetwork::new(
                z_session,
                shard_state.clone(),
                prefix,
            ))),
            token: CancellationToken::new(),
        }
    }

    fn sync_network(&self) {
        let wrapper = self.clone();
        tokio::spawn(async move {
            let mut receiver = wrapper.shard_state.watch_readiness();
            loop {
                tokio::select! {
                    _ = receiver.changed() => {
                        let mut network = wrapper.network.lock().await;
                        if receiver.borrow().to_owned() {
                            if !network.is_running() {
                                info!("Start network for shard {}", wrapper.shard_state.meta().id);
                                if let Err(e) = network.start().await {
                                    error!("Failed to start network: {}", e);
                                }
                            }
                        } else {
                            if network.is_running() {
                                network.stop();
                            }
                        }
                    }
                    _ = wrapper.token.cancelled() => {
                        break;
                    }
                }
            }
        });
    }

    async fn close(&self) -> Result<(), FlareError> {
        self.token.cancel();
        let network = self.network.lock().await;
        if network.is_running() {
            network.stop();
        }
        self.shard_state.close().await
    }
}

impl std::ops::Deref for Shard {
    type Target = ObjectShardState;

    fn deref(&self) -> &Self::Target {
        &self.shard_state
    }
}
