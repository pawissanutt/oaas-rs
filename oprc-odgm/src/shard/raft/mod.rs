mod generic;
mod state_machine;

use flare_dht::{
    error::FlareError,
    metadata::raft::rpc::Network,
    raft::log::MemLogStore,
    shard::{KvShard, ShardMetadata},
};
use generic::LocalStateMachineStore;
use state_machine::{ShardReq, ShardResp};
use std::{io::Cursor, sync::Arc};

use super::ObjectEntry;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = ShardReq,
        R = ShardResp,
);

#[derive(Clone)]
pub struct RaftObjectShard {
    pub(crate) shard_metadata: ShardMetadata,
    raft: openraft::Raft<TypeConfig>,
    raft_config: Arc<openraft::Config>,
}

impl RaftObjectShard {
    pub async fn new(
        z_session: zenoh::Session,
        rpc_prefix: String,
        shard_metadata: ShardMetadata,
    ) -> Self {
        let config = openraft::Config {
            ..Default::default()
        };
        let config = Arc::new(config.validate().unwrap());
        let log_store = MemLogStore::default();
        let sm: LocalStateMachineStore<
            state_machine::ObjectShardStateMachine,
            TypeConfig,
        > = LocalStateMachineStore::default();
        let network = Network::new(z_session.clone(), rpc_prefix.into());
        let raft = openraft::Raft::<TypeConfig>::new(
            shard_metadata.id,
            config.clone(),
            network,
            log_store.clone(),
            sm.clone(),
        )
        .await
        .unwrap();

        Self {
            shard_metadata,
            raft,
            raft_config: config,
        }
    }
}

#[async_trait::async_trait]
impl KvShard for RaftObjectShard {
    type Key = u64;
    type Entry = ObjectEntry;

    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, FlareError> {
        todo!()
    }

    // async fn modify<F, O>(
    //     &self,
    //     key: &Self::Key,
    //     processor: F,
    // ) -> Result<O, FlareError>
    // where
    //     F: FnOnce(&mut Self::Entry) -> O + Send,
    // {
    //     todo!()
    // }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), FlareError> {
        todo!()
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError> {
        todo!()
    }
}
