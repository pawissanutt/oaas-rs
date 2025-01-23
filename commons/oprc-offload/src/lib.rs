use std::{sync::Arc, time::Duration};

use conn::{ConnManager, PoolConfig};
use grpc::RpcManager;
use mobc::Connection;
use route::{Routable, RoutingManager};
use tonic::Status;
use tracing::info;

pub mod conn;
pub mod grpc;
pub mod proxy;
pub mod route;

#[derive(Clone)]
pub struct Invoker {
    conn: Arc<ConnManager<Routable, RpcManager>>,
    routing_manager: Arc<route::RoutingManager>,
}

impl Invoker {
    pub fn new(config: PoolConfig) -> Self {
        let routing_manager = Arc::new(RoutingManager::new());

        let conn = Arc::new(ConnManager::new(routing_manager.clone(), config));
        Self {
            conn,
            routing_manager,
        }
    }

    pub async fn start_sync(&self, addr: &str) -> Result<(), OffloadError> {
        self.routing_manager.clone().start_sync(addr).await
    }

    pub async fn get_conn(
        &self,
        key: Routable,
    ) -> Result<Connection<RpcManager>, mobc::Error<OffloadError>> {
        self.conn.get(key).await
    }

    pub fn print_pool_state_interval(&self, interval: u64) {
        let cm = self.conn.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval as u64)).await;
                let states = cm.get_states().await;
                info!("pool state {:?}", states);
            }
        });
    }
}

#[derive(thiserror::Error, Debug)]
pub enum OffloadError {
    #[error("gRPC error: {0}")]
    GrpcError(#[from] Status),
    #[error("gRPC error: {0}")]
    GrpcConnectError(#[from] tonic::transport::Error),
    #[error("Uri parsing error: {0}")]
    InvalidUrl(#[from] http::uri::InvalidUri),
    #[error("No class {0} exists")]
    NoCls(String),
    #[error("No func {1} on class {0} exists")]
    NoFunc(String, String),
    #[error("No partition {1} on class {0} exists")]
    NoPartition(String, u16),
    #[error("Pool error: {0}")]
    PoolError(String),
}

impl From<mobc::Error<OffloadError>> for OffloadError {
    fn from(value: mobc::Error<OffloadError>) -> Self {
        match value {
            mobc::Error::Inner(e) => e,
            mobc::Error::Timeout => {
                OffloadError::GrpcError(Status::deadline_exceeded("timeout"))
            }
            mobc::Error::BadConn => {
                OffloadError::GrpcError(Status::unavailable("bad connection"))
            }
            mobc::Error::PoolClosed => {
                OffloadError::GrpcError(Status::unavailable("pool closed"))
            }
        }
    }
}
