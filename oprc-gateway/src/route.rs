use std::str::FromStr;

use http::Uri;
use oprc_pb::{
    routing_service_client::RoutingServiceClient, ClsRouting, ClsRoutingRequest,
};
use tonic::{transport::Channel, Request};
use tracing::info;

use crate::{conn::ConnFactory, error::GatewayError, rpc::RpcManager};

#[derive(Clone, Debug)]
pub struct RoutingManager {
    table: dashmap::DashMap<String, ClsRouting>,
}

impl RoutingManager {
    pub fn new() -> RoutingManager {
        RoutingManager {
            table: dashmap::DashMap::new(),
        }
    }

    pub async fn start_sync(&self, addr: &str) -> Result<(), GatewayError> {
        let uri = Uri::from_str(addr)?;
        let channel = Channel::builder(uri).connect().await?;
        let mut client = RoutingServiceClient::new(channel);
        let resp = client
            .get_cls_routing(Request::new(ClsRoutingRequest {}))
            .await?;
        let table = resp.into_inner();
        for cls_routing in table.clss {
            info!("update routing table: cls={}", cls_routing.name);
            self.table.insert(cls_routing.name.clone(), cls_routing);
        }
        Ok(())
    }

    pub fn get_route(
        &self,
        cls: &str,
        func: &str,
    ) -> Result<String, GatewayError> {
        if let Some(routing) = self.table.get(cls) {
            if let Some(partition) = routing.routings.get(0) {
                if let Some(f_route) = partition.funcs.get(func) {
                    return Ok(f_route.uri.clone());
                }
            }
            return Err(GatewayError::NoFunc(cls.into(), func.into()));
        }
        Err(GatewayError::NoCls(cls.into()))
    }
}

#[async_trait::async_trait]
impl ConnFactory<Routable, RpcManager> for RoutingManager {
    async fn create(&self, key: Routable) -> Result<RpcManager, GatewayError> {
        let uri = self.get_route(&key.cls, &key.func)?;
        Ok(RpcManager::new(&uri)?)
    }
}

#[derive(Hash, Clone, PartialEq, Eq, Debug)]
pub struct Routable {
    pub cls: String,
    pub func: String,
}
