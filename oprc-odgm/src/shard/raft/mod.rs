mod rpc;
mod state_machine;

use flare_dht::raft::generic::LocalStateMachineStore;
use flare_dht::raft::{
    log::MemLogStore,
    rpc::{Network, RaftZrpcService},
};
use flare_zrpc::client::ZrpcClientConfig;
use flare_zrpc::server::ServerConfig;
use rpc::{RaftOperationHandler, RaftOperationManager, RaftOperationService};
use state_machine::{ObjectShardStateMachine, ShardReq, ShardResp};
use std::collections::BTreeMap;
use std::{io::Cursor, sync::Arc};
use tokio::sync::watch::Receiver;
use tokio::sync::watch::Sender;
use tokio::sync::Mutex;
use tracing::{info, warn};
use zenoh::qos::{CongestionControl, Priority};

use super::{ObjectEntry, ShardMetadata, ShardState};
use crate::error::OdgmError;

openraft::declare_raft_types!(
    pub TypeConfig:
        D = ShardReq,
        R = ShardResp,
);

pub struct RaftObjectShard {
    pub(crate) shard_metadata: ShardMetadata,
    store: LocalStateMachineStore<ObjectShardStateMachine, TypeConfig>,
    raft: openraft::Raft<TypeConfig>,
    // raft_config: Arc<openraft::Config>,
    rpc_service: Mutex<RaftZrpcService<TypeConfig>>,
    operation_manager: RaftOperationManager<TypeConfig>,
    operation_service: Mutex<RaftOperationService<TypeConfig>>,
    readiness_sender: Sender<bool>,
    readiness_receiver: Receiver<bool>,
    cancellation: tokio_util::sync::CancellationToken,
}

impl RaftObjectShard {
    pub async fn new(
        z_session: zenoh::Session,
        rpc_prefix: String,
        shard_metadata: ShardMetadata,
    ) -> Self {
        let config = openraft::Config {
            cluster_name: format!(
                "oprc/{}/{}",
                shard_metadata.collection, shard_metadata.partition_id,
            ),
            election_timeout_min: 200,
            election_timeout_max: 2000,
            max_payload_entries: 1024,
            purge_batch_size: 1024,
            heartbeat_interval: 100,
            snapshot_policy: openraft::SnapshotPolicy::LogsSinceLast(100000),
            ..Default::default()
        };
        let config = Arc::new(config.validate().unwrap());
        let log_store = MemLogStore::default();
        let store: LocalStateMachineStore<ObjectShardStateMachine, TypeConfig> =
            LocalStateMachineStore::default();
        let zrpc_client_config = ZrpcClientConfig {
            service_id: rpc_prefix.to_owned(),
            target: zenoh::query::QueryTarget::BestMatching,
            channel_size: 8,
            congestion_control: CongestionControl::Drop,
            priority: Priority::DataHigh,
        };
        let zrpc_server_config = ServerConfig {
            reply_congestion: CongestionControl::Block,
            reply_priority: Priority::DataHigh,
            ..Default::default()
        };
        let network = Network::new_with_config(
            z_session.clone(),
            rpc_prefix.to_owned(),
            zrpc_client_config.clone(),
        );
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
            zrpc_server_config.clone(),
        );

        let operation_manager = RaftOperationManager::new(
            raft.clone(),
            z_session.clone(),
            format!("{rpc_prefix}/ops"),
            shard_metadata.id,
            zrpc_client_config,
        )
        .await;

        let conf = ServerConfig {
            service_id: format!("{rpc_prefix}/ops/{}", shard_metadata.id),
            ..zrpc_server_config
        };
        let operation_service = RaftOperationService::new(
            z_session,
            conf,
            RaftOperationHandler::new(raft.clone()),
        );

        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            shard_metadata,
            raft,
            // raft_config: config,
            store,
            rpc_service: Mutex::new(rpc_service),
            operation_manager,
            operation_service: Mutex::new(operation_service),
            readiness_sender: tx,
            readiness_receiver: rx,
            cancellation: tokio_util::sync::CancellationToken::new(),
        }
    }
}

#[async_trait::async_trait]
impl ShardState for RaftObjectShard {
    type Key = u64;
    type Entry = ObjectEntry;

    #[inline]
    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    #[inline]
    fn watch_readiness(&self) -> Receiver<bool> {
        self.readiness_receiver.clone()
    }

    async fn initialize(&self) -> Result<(), OdgmError> {
        if let Err(e) = self.rpc_service.lock().await.start().await {
            return Err(OdgmError::UnknownError(e));
        }
        if let Err(e) = self.operation_service.lock().await.start().await {
            return Err(OdgmError::UnknownError(e));
        }

        let leader_only = self
            .shard_metadata
            .options
            .get("raft_net_leader_only")
            .map(|v| v == "true")
            .unwrap_or(false);

        if leader_only {
            let sender = self.readiness_sender.clone();
            let token = self.cancellation.clone();
            let mut watch = self.raft.server_metrics();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = token.cancelled() => {
                            break;
                        },
                        res = watch.changed() => {
                            if let Err(_) = res {
                                break;
                            }
                            let metric = watch.borrow();
                            let _ = sender.send(Some(metric.id) == metric.current_leader);
                        }
                    }
                }
            });
        } else {
            let _ = self.readiness_sender.send(true);
        }

        let init_leader_only = self
            .shard_metadata
            .options
            .get("raft_init_leader_only")
            .map(|v| v == "true")
            .unwrap_or(false);

        if !init_leader_only
            || self.shard_metadata.primary == Some(self.shard_metadata.id)
        {
            info!(
                "shard '{}': initialize raft cluster",
                self.shard_metadata.id
            );
            let mut members = BTreeMap::new();
            for member in self.shard_metadata.replica.iter() {
                members.insert(
                    *member,
                    openraft::BasicNode {
                        addr: member.to_string(),
                    },
                );
            }
            if let Err(e) = self.raft.initialize(members).await {
                warn!(
                    "shard '{}': error initiating raft: {:?}",
                    self.shard_metadata.id, e
                );
            }
        }

        Ok(())
    }

    async fn close(&mut self) -> Result<(), OdgmError> {
        self.rpc_service.lock().await.close().await;
        self.operation_service.lock().await.close().await;
        self.cancellation.cancel();
        if let Err(e) = self.raft.shutdown().await {
            tracing::error!(
                "shard '{}': error shutting down raft: {:?}",
                self.shard_metadata.id,
                e
            );
        }
        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, OdgmError> {
        let sm = self.store.state_machine.read().await;
        let out = sm.data.get(&key).cloned();
        Ok(out)
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), OdgmError> {
        let req = ShardReq::Set(key, value);
        let _ = self
            .operation_manager
            .exec(&req)
            .await
            .map_err(|e| OdgmError::UnknownError(Box::new(e)))?;
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError> {
        let req = ShardReq::Delete(*key);
        let _ = self
            .operation_manager
            .exec(&req)
            .await
            .map_err(|e| OdgmError::UnknownError(Box::new(e)))?;
        Ok(())
    }
    async fn count(&self) -> Result<u64, OdgmError> {
        Ok(self.store.state_machine.read().await.data.len() as u64)
    }
}

#[cfg(test)]
mod test {
    use crate::shard::{ShardMetadata, ShardState};

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_single_shard() {
        let z_session =
            zenoh::open(zenoh::config::Config::default()).await.unwrap();
        let rpc_prefix = "test".to_string();
        let shard_metadata = ShardMetadata {
            id: 1,
            collection: "test".to_string(),
            partition_id: 1,
            owner: Some(1),
            primary: Some(1),
            replica_owner: vec![1],
            replica: vec![1],
            ..Default::default()
        };
        let mut shard =
            super::RaftObjectShard::new(z_session, rpc_prefix, shard_metadata)
                .await;
        shard.initialize().await.unwrap();
        let obj = super::ObjectEntry::random(10);
        shard.set(1, obj.clone()).await.unwrap();
        let out = shard.get(&1).await.unwrap();
        assert_eq!(out, Some(obj));
        shard.delete(&1).await.unwrap();
        let out = shard.get(&1).await.unwrap();
        assert_eq!(out, None);
        shard.close().await.unwrap();
    }
}
