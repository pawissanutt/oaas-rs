use mobc::Manager;
use oprc_grpc::oprc_function_client::OprcFunctionClient;
use std::str::FromStr;
use tonic::transport::Channel;
use tonic::transport::Uri;
use tracing::debug;
use tracing::info;

use crate::error::GatewayError;

#[derive(Debug)]
pub struct RpcManager {
    uri: Uri,
}

impl RpcManager {
    pub fn new(addr: &str) -> Result<Self, GatewayError> {
        let uri = Uri::from_str(addr)?;
        info!("create RPC manager for '{}'", addr);
        Ok(Self { uri })
    }
}

#[async_trait::async_trait]
impl Manager for RpcManager {
    type Connection = OprcFunctionClient<Channel>;

    type Error = GatewayError;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let channel = Channel::builder(self.uri.clone()).connect().await?;
        let client = OprcFunctionClient::new(channel);
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
