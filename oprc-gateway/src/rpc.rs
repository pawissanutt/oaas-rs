use mobc::Manager;
use oprc_pb::oprc_function_client::OprcFunctionClient;
use std::str::FromStr;
use tonic::transport::channel;
use tonic::transport::Channel;
use tonic::transport::Uri;
use tracing::debug;
use tracing::info;

use crate::error::GatewayError;

#[derive(Debug)]
pub struct RpcManager {
    uri: Uri,
    // channel: Channel,
}

impl RpcManager {
    pub fn new(addr: &str) -> Result<Self, GatewayError> {
        let uri = Uri::from_str(addr)?;
        // let ch = Channel::builder(uri.clone()).buffer_size(65536);
        // let list = [ch].into_iter();
        // let channel = Channel::balance_list(list);
        // let channel = Channel::builder(uri.clone()).connect_lazy();
        info!("create RPC manager for '{}'", addr);
        Ok(Self {
            uri,
            // channel
        })
    }
}

#[async_trait::async_trait]
impl Manager for RpcManager {
    type Connection = OprcFunctionClient<Channel>;

    type Error = GatewayError;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let channel = Channel::builder(self.uri.clone())
            // .buffer_size(65536)
            .connect()
            .await?;
        let client = OprcFunctionClient::new(channel);
        // let client = OprcFunctionClient::new(self.channel.clone());

        debug!("create new client for '{:?}'", self.uri);
        Ok(client)
    }

    async fn check(
        &self,
        conn: Self::Connection,
    ) -> Result<Self::Connection, Self::Error> {
        Ok(conn)
    }
}
