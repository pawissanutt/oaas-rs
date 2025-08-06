use std::error::Error;

use flare_zrpc::{
    bincode::BincodeZrpcType,
    client::ZrpcClientConfig,
    server::{ServerConfig, ZrpcService},
    ZrpcClient, ZrpcError, ZrpcServiceHander,
};
use openraft::anyerror::AnyError;
use openraft::{
    error::{
        InstallSnapshotError, NetworkError, RPCError, RaftError, RemoteError,
        Unreachable,
    },
    network::RPCOption,
    raft::{
        AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest,
        InstallSnapshotResponse, VoteRequest, VoteResponse,
    },
    Raft, RaftNetwork, RaftNetworkFactory, RaftTypeConfig,
};
use zenoh::Session;

#[allow(type_alias_bounds)]
type AppendType<C: RaftTypeConfig> = BincodeZrpcType<
    AppendEntriesRequest<C>,
    AppendEntriesResponse<C::NodeId>,
    RaftError<C::NodeId>,
>;

pub struct AppendHandler<T: RaftTypeConfig> {
    raft: Raft<T>,
}

#[async_trait::async_trait]
impl<C: RaftTypeConfig> ZrpcServiceHander<AppendType<C>> for AppendHandler<C> {
    async fn handle(
        &self,
        req: AppendEntriesRequest<C>,
    ) -> Result<AppendEntriesResponse<C::NodeId>, RaftError<C::NodeId>> {
        self.raft.append_entries(req).await
    }
}

#[allow(type_alias_bounds)]
type VoteType<C: RaftTypeConfig> = BincodeZrpcType<
    VoteRequest<C::NodeId>,
    VoteResponse<C::NodeId>,
    RaftError<C::NodeId>,
>;
pub struct VoteHandler<T: RaftTypeConfig> {
    raft: Raft<T>,
}

#[async_trait::async_trait]
impl<C: RaftTypeConfig> ZrpcServiceHander<VoteType<C>> for VoteHandler<C> {
    async fn handle(
        &self,
        req: VoteRequest<C::NodeId>,
    ) -> Result<VoteResponse<C::NodeId>, RaftError<C::NodeId>> {
        self.raft.vote(req).await
    }
}

#[allow(type_alias_bounds)]
type InstallSnapshotType<C: RaftTypeConfig> = BincodeZrpcType<
    InstallSnapshotRequest<C>,
    InstallSnapshotResponse<C::NodeId>,
    RaftError<C::NodeId, InstallSnapshotError>,
>;

pub struct InstallSnapshotHandler<T: RaftTypeConfig> {
    raft: Raft<T>,
}

#[async_trait::async_trait]
impl<C: RaftTypeConfig> ZrpcServiceHander<InstallSnapshotType<C>>
    for InstallSnapshotHandler<C>
{
    async fn handle(
        &self,
        req: InstallSnapshotRequest<C>,
    ) -> Result<
        InstallSnapshotResponse<C::NodeId>,
        RaftError<C::NodeId, InstallSnapshotError>,
    > {
        self.raft.install_snapshot(req).await
    }
}

#[allow(type_alias_bounds)]
pub type AppendService<C: RaftTypeConfig> =
    ZrpcService<AppendHandler<C>, AppendType<C>>;
#[allow(type_alias_bounds)]
pub type VoteService<C: RaftTypeConfig> =
    ZrpcService<VoteHandler<C>, VoteType<C>>;
#[allow(type_alias_bounds)]
pub type SnapshotService<C: RaftTypeConfig> =
    ZrpcService<InstallSnapshotHandler<C>, InstallSnapshotType<C>>;

#[derive(Clone)]
pub struct RaftZrpcService<C: RaftTypeConfig> {
    append: AppendService<C>,
    vote: VoteService<C>,
    snapshot: SnapshotService<C>,
}

impl<C: RaftTypeConfig> RaftZrpcService<C> {
    pub fn new(
        raft: Raft<C>,
        z_session: Session,
        rpc_prefix: String,
        node_id: C::NodeId,
        conf: ServerConfig,
    ) -> Self {
        let append_conf = ServerConfig {
            service_id: format!("{rpc_prefix}/raft-append/{node_id}"),
            ..conf
        };
        let append = AppendService::new(
            z_session.clone(),
            append_conf,
            AppendHandler { raft: raft.clone() },
        );
        let vote_conf = ServerConfig {
            service_id: format!("{rpc_prefix}/raft-vote/{node_id}"),
            ..conf
        };
        let vote = VoteService::new(
            z_session.clone(),
            vote_conf,
            VoteHandler { raft: raft.clone() },
        );
        let snapshot_conf = ServerConfig {
            service_id: format!("{rpc_prefix}/raft-snapshot/{node_id}"),
            ..conf
        };
        let snapshot = SnapshotService::new(
            z_session.clone(),
            snapshot_conf,
            InstallSnapshotHandler { raft: raft.clone() },
        );

        Self {
            append,
            vote,
            snapshot,
        }
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn Error + Sync + Send>> {
        self.append.start().await?;
        self.vote.start().await?;
        self.snapshot.start().await?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn close(&mut self) {
        self.append.close().await;
        self.vote.close().await;
        self.snapshot.close().await;
    }
}

pub struct Network {
    z_session: Session,
    rpc_prefix: String,
    config: ZrpcClientConfig,
}

impl Network {
    pub fn new(
        z_session: Session,
        rpc_prefix: String,
        config: ZrpcClientConfig,
    ) -> Self {
        Network {
            z_session,
            rpc_prefix,
            config,
        }
    }
}

impl<C: RaftTypeConfig> RaftNetworkFactory<C> for Network {
    type Network = NetworkConnection<C>;

    async fn new_client(
        &mut self,
        target: C::NodeId,
        _node: &C::Node,
    ) -> NetworkConnection<C> {
        NetworkConnection::new(
            self.z_session.clone(),
            self.rpc_prefix.clone(),
            target,
            self.config.clone(),
        )
        .await
    }
}

#[allow(type_alias_bounds)]
type AppendClient<C: RaftTypeConfig> = ZrpcClient<AppendType<C>>;

#[allow(type_alias_bounds)]
type VoteClient<C: RaftTypeConfig> = ZrpcClient<VoteType<C>>;

#[allow(type_alias_bounds)]
type InstallSnapshotClient<C: RaftTypeConfig> =
    ZrpcClient<InstallSnapshotType<C>>;

pub struct NetworkConnection<C: RaftTypeConfig> {
    target: C::NodeId,
    append_client: AppendClient<C>,
    vote_client: VoteClient<C>,
    snapshot_client: InstallSnapshotClient<C>,
}

impl<C: RaftTypeConfig> NetworkConnection<C> {
    async fn new(
        z_session: Session,
        rpc_prefix: String,
        target: C::NodeId,
        config: ZrpcClientConfig,
    ) -> Self {
        let append_client = AppendClient::with_config(
            ZrpcClientConfig {
                service_id: format!("{rpc_prefix}/raft-append/{target}"),
                ..config
            },
            z_session.clone(),
        )
        .await;
        let vote_client = VoteClient::<C>::with_config(
            ZrpcClientConfig {
                service_id: format!("{rpc_prefix}/raft-vote/{target}"),
                ..config
            },
            z_session.clone(),
        )
        .await;

        let snapshot_client = InstallSnapshotClient::with_config(
            ZrpcClientConfig {
                service_id: format!("{rpc_prefix}/raft-snapshot/{target}"),
                ..config
            },
            z_session.clone(),
        )
        .await;
        Self {
            target,
            append_client,
            vote_client,
            snapshot_client,
        }
    }

    fn convert<E: Error + 'static>(
        &self,
        error: ZrpcError<E>,
    ) -> RPCError<C::NodeId, C::Node, E> {
        match error {
            ZrpcError::AppError(app_err) => RPCError::RemoteError(
                RemoteError::new(self.target.to_owned(), app_err),
            ),
            ZrpcError::ConnectionError(err) => RPCError::Unreachable(
                Unreachable::from(AnyError::error(err.to_string())),
            ),
            err => RPCError::Network(NetworkError::from(AnyError::new(&err))),
        }
    }
}

impl<C: RaftTypeConfig> RaftNetwork<C> for NetworkConnection<C> {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<C>,
        _option: RPCOption,
    ) -> Result<
        AppendEntriesResponse<C::NodeId>,
        RPCError<C::NodeId, C::Node, RaftError<C::NodeId>>,
    > {
        let res = self.append_client.call(&req).await;
        res.map_err(|e| self.convert(e))
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<C>,
        _option: RPCOption,
    ) -> Result<
        InstallSnapshotResponse<C::NodeId>,
        RPCError<
            C::NodeId,
            C::Node,
            RaftError<C::NodeId, InstallSnapshotError>,
        >,
    > {
        let res = self.snapshot_client.call(&req).await;
        res.map_err(|e| self.convert(e))
    }

    async fn vote(
        &mut self,
        req: VoteRequest<C::NodeId>,
        _option: RPCOption,
    ) -> Result<
        VoteResponse<C::NodeId>,
        RPCError<C::NodeId, C::Node, RaftError<C::NodeId>>,
    > {
        let res = self.vote_client.call(&req).await;
        res.map_err(|e| self.convert(e))
    }
}
