use flare_zrpc::{
    bincode::BincodeZrpcType, ZrpcClient, ZrpcError, ZrpcService,
    ZrpcServiceHander,
};
use openraft::{
    error::{ClientWriteError, ForwardToLeader, RaftError},
    raft::ClientWriteResponse,
    Raft, RaftTypeConfig,
};
use tokio::sync::RwLock;

#[allow(type_alias_bounds)]
pub type RaftOperationType<C: RaftTypeConfig> = BincodeZrpcType<
    C::D,
    ClientWriteResponse<C>,
    RaftError<C::NodeId, ClientWriteError<C::NodeId, C::Node>>,
>;

#[allow(type_alias_bounds)]
pub type RaftOperationService<C: RaftTypeConfig> =
    ZrpcService<RaftOperationHandler<C>, RaftOperationType<C>>;

pub struct RaftOperationHandler<C: RaftTypeConfig> {
    raft: Raft<C>,
}

impl<C: RaftTypeConfig> RaftOperationHandler<C> {
    pub fn new(raft: Raft<C>) -> Self {
        Self { raft }
    }
}

#[async_trait::async_trait]
impl<C: RaftTypeConfig> ZrpcServiceHander<RaftOperationType<C>>
    for RaftOperationHandler<C>
where
    C: RaftTypeConfig<Responder = openraft::impls::OneshotResponder<C>>,
{
    async fn handle(
        &self,
        req: C::D,
    ) -> Result<
        ClientWriteResponse<C>,
        RaftError<C::NodeId, ClientWriteError<C::NodeId, C::Node>>,
    > {
        self.raft.client_write(req).await
    }
}

pub struct RaftOperationManager<C>
where
    C: RaftTypeConfig<Responder = openraft::impls::OneshotResponder<C>>,
{
    local_handler: RaftOperationHandler<C>,
    rpc: ZrpcClient<RaftOperationType<C>>,
    local: u64,
    leader: tokio::sync::RwLock<u64>,
}

impl<C> RaftOperationManager<C>
where
    C: RaftTypeConfig<
        D: Clone,
        Responder = openraft::impls::OneshotResponder<C>,
        NodeId = u64,
    >,
{
    pub async fn new(
        raft: Raft<C>,
        z_session: zenoh::Session,
        prefix: String,
        node_id: u64,
    ) -> Self {
        let rpc = RaftOperationHandler { raft: raft };
        Self {
            local_handler: rpc,
            rpc: ZrpcClient::new(prefix, z_session).await,
            local: node_id,
            leader: RwLock::new(node_id),
        }
    }

    async fn do_exec(
        &self,
        op: &C::D,
    ) -> Result<
        ClientWriteResponse<C>,
        RaftError<C::NodeId, ClientWriteError<C::NodeId, C::Node>>,
    > {
        let key = *self.leader.read().await;

        if self.local == key {
            self.exec_local(op).await
        } else {
            self.forward_to_leader(key, op).await
        }
    }

    async fn exec_local(
        &self,
        op: &C::D,
    ) -> Result<
        ClientWriteResponse<C>,
        RaftError<C::NodeId, ClientWriteError<C::NodeId, C::Node>>,
    > {
        match self.local_handler.handle(op.to_owned()).await {
            Ok(resp) => Ok(resp),
            Err(e) => {
                if let RaftError::APIError(ClientWriteError::ForwardToLeader(
                    ForwardToLeader {
                        leader_id: Some(leader_id),
                        ..
                    },
                )) = e
                {
                    *self.leader.write().await = leader_id;
                }
                Err(e)
            }
        }
    }

    async fn forward_to_leader(
        &self,
        key: u64,
        op: &C::D,
    ) -> Result<
        ClientWriteResponse<C>,
        RaftError<C::NodeId, ClientWriteError<C::NodeId, C::Node>>,
    > {
        let key_str = format!("{key}");
        match self.rpc.call_with_key(key_str, op).await {
            Ok(resp) => Ok(resp),
            Err(ZrpcError::AppError(e)) => {
                if let RaftError::APIError(ClientWriteError::ForwardToLeader(
                    ForwardToLeader {
                        leader_id: Some(leader_id),
                        ..
                    },
                )) = e
                {
                    *self.leader.write().await = leader_id;
                }
                Err(e)
            }
            Err(e) => {
                tracing::error!("error in rpc call: {:?}", e);
                Err(RaftError::Fatal(openraft::error::Fatal::Panicked))
            }
        }
    }

    pub async fn exec(
        &self,
        op: &C::D,
    ) -> Result<
        ClientWriteResponse<C>,
        RaftError<C::NodeId, ClientWriteError<C::NodeId, C::Node>>,
    > {
        let mut count = 3;
        loop {
            let result = self.do_exec(op).await;
            if let Ok(resp) = result {
                return Ok(resp);
            } else {
                count -= 1;
                if count <= 0 {
                    return result;
                }
            };
        }
    }
}
