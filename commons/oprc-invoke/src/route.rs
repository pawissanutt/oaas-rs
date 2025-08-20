use std::str::FromStr;
use std::sync::Arc;

use http::Uri;
use oprc_pb::{
    ClsRouting, ClsRoutingRequest, routing_service_client::RoutingServiceClient,
};
use tonic::{Request, transport::Channel};
use tracing::{info, warn};

use crate::{conn::ConnFactory, grpc::RpcManager};

use super::OffloadError;

#[derive(Clone, Debug)]
pub struct RoutingManager {
    table: scc::HashMap<String, ClsRouting>,
}

impl RoutingManager {
    pub fn new() -> RoutingManager {
        RoutingManager {
            table: scc::HashMap::new(),
        }
    }

    pub async fn start_sync(
        self: Arc<Self>,
        addr: &str,
    ) -> Result<(), OffloadError> {
        let uri = Uri::from_str(addr)?;
        let channel = Channel::builder(uri).connect().await?;
        let mut client = RoutingServiceClient::new(channel);
        info!("start pulling routing table");
        let resp = client
            .get_cls_routing(Request::new(ClsRoutingRequest {}))
            .await?;
        let table = resp.into_inner();
        for cls_routing in table.clss {
            info!(
                "update routing table: cls={:?}, {:?}",
                cls_routing.name, cls_routing.routing
            );
            let entry =
                self.table.entry_async(cls_routing.name.to_owned()).await;
            entry.insert_entry(cls_routing);
        }
        tokio::spawn(async move {
            loop {
                let resp = client
                    .watch_cls_routing(Request::new(ClsRoutingRequest {}))
                    .await;
                if let Err(e) = resp {
                    warn!("error on watching routing table: {}", e);
                    continue;
                }
                let mut streaming = resp.unwrap().into_inner();
                loop {
                    match streaming.message().await {
                        Ok(item) => {
                            if let Some(cls_routing) = item {
                                info!(
                                    "update routing table: cls={:?}, {:?}",
                                    cls_routing.name, cls_routing.routing
                                );
                                let entry = self
                                    .table
                                    .entry_async(cls_routing.name.to_owned())
                                    .await;
                                entry.insert_entry(cls_routing);
                            } else {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("error on watching routing table: {}", e)
                        }
                    }
                }
            }
        });
        Ok(())
    }

    pub fn get_route(
        &self,
        routable: &Routable,
    ) -> Result<String, OffloadError> {
        if let Some(cls_routing) = self.table.get(&routable.cls) {
            if let Some(partition) =
                cls_routing.routing.get(routable.partition as usize)
            {
                if let Some(f_route) = partition.functions.get(&routable.func) {
                    return Ok(f_route.url.clone());
                }

                return Err(OffloadError::NoFunc(
                    routable.cls.to_owned(),
                    routable.func.to_owned(),
                ));
            }

            return Err(OffloadError::NoPartition(
                String::from(&routable.cls),
                routable.partition,
            ));
        }
        Err(OffloadError::NoCls(String::from(&routable.cls)))
    }
}

#[async_trait::async_trait]
impl ConnFactory<Routable, RpcManager> for RoutingManager {
    async fn create(&self, key: Routable) -> Result<RpcManager, OffloadError> {
        let uri = self.get_route(&key)?;
        Ok(RpcManager::new(&uri)?)
    }
}

#[derive(Hash, Clone, PartialEq, Eq, Debug, Default)]
pub struct Routable {
    pub cls: String,
    pub func: String,
    pub partition: u16,
}
