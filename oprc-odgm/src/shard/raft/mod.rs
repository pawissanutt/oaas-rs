mod rpc;
mod state_machine;

use flare_dht::raft::generic::LocalStateMachineStore;
use flare_dht::{
    error::FlareError,
    raft::{
        log::MemLogStore,
        rpc::{Network, RaftZrpcService},
    },
    shard::{KvShard, ShardMetadata},
};
use rpc::{RaftOperationHandler, RaftOperationManager, RaftOperationService};
use state_machine::{ObjectShardStateMachine, ShardReq, ShardResp};
use std::{io::Cursor, sync::Arc};

use super::ObjectEntry;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = ShardReq,
        R = ShardResp,
);

pub struct RaftObjectShard {
    pub(crate) shard_metadata: ShardMetadata,
    store: LocalStateMachineStore<ObjectShardStateMachine, TypeConfig>,
    raft: openraft::Raft<TypeConfig>,
    raft_config: Arc<openraft::Config>,
    rpc_service: RaftZrpcService<TypeConfig>,
    operation_manager: RaftOperationManager<TypeConfig>,
    operation_service: RaftOperationService<TypeConfig>,
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
        let store: LocalStateMachineStore<ObjectShardStateMachine, TypeConfig> =
            LocalStateMachineStore::default();
        let network = Network::new(z_session.clone(), rpc_prefix.to_owned());
        let raft = openraft::Raft::<TypeConfig>::new(
            shard_metadata.id,
            config.clone(),
            network,
            log_store.clone(),
            store.clone(),
        )
        .await
        .unwrap();
        let rpc_service = RaftZrpcService::new(
            raft.clone(),
            z_session.clone(),
            rpc_prefix.clone(),
            shard_metadata.id,
        );

        let operation_manager = RaftOperationManager::new(
            raft.clone(),
            z_session.clone(),
            format!("{rpc_prefix}/ops"),
            shard_metadata.id,
        )
        .await;

        let operation_service = RaftOperationService::new(
            format!("{rpc_prefix}/ops/{}", shard_metadata.id),
            z_session,
            RaftOperationHandler::new(raft.clone()),
        );

        Self {
            shard_metadata,
            raft,
            raft_config: config,
            store,
            rpc_service,
            operation_manager,
            operation_service,
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

    async fn initialize(&self) -> Result<(), FlareError> {
        if let Err(e) = self.rpc_service.start().await {
            return Err(FlareError::UnknownError(e));
        }
        Ok(())
    }

    async fn close(&self) -> Result<(), FlareError> {
        self.rpc_service.close();
        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, FlareError> {
        let sm = self.store.state_machine.read().await;
        let out = sm.data.get(&key).cloned();
        Ok(out)
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), FlareError> {
        let req = ShardReq::Set(key, value);
        let _ = self
            .operation_manager
            .exec(&req)
            .await
            .map_err(|e| FlareError::UnknownError(Box::new(e)))?;
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError> {
        let req = ShardReq::Delete(*key);
        let _ = self
            .operation_manager
            .exec(&req)
            .await
            .map_err(|e| FlareError::UnknownError(Box::new(e)))?;
        Ok(())
    }
}
