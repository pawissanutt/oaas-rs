use mobc::Manager;
use oprc_grpc::oprc_function_client::OprcFunctionClient;
use std::str::FromStr;
use tonic::transport::Channel;
use tonic::transport::Uri;
use tracing::debug;
use tracing::info;

use crate::OffloadError;

#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub max_encoding_message_size: usize,
    pub max_decoding_message_size: usize,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            max_encoding_message_size: usize::MAX,
            max_decoding_message_size: usize::MAX,
        }
    }
}

#[derive(Debug)]
pub struct RpcManager {
    uri: Uri,
    conf: RpcConfig,
}

impl RpcManager {
    pub fn new(addr: &str) -> Result<Self, http::uri::InvalidUri> {
        let uri = Uri::from_str(addr)?;
        info!("create RPC manager for '{}'", addr);
        Ok(Self {
            uri,
            conf: RpcConfig::default(),
        })
    }

    pub fn new_with_conf(
        addr: &str,
        conf: RpcConfig,
    ) -> Result<Self, http::uri::InvalidUri> {
        let uri = Uri::from_str(addr)?;
        info!("create RPC manager for '{}'", addr);
        Ok(Self { uri, conf })
    }
}

#[async_trait::async_trait]
impl Manager for RpcManager {
    type Connection = OprcFunctionClient<Channel>;

    type Error = OffloadError;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let channel = Channel::builder(self.uri.clone())
            // .timeout(self.conf.timeout)
            .connect()
            .await?;
        let client = OprcFunctionClient::new(channel)
            .max_decoding_message_size(self.conf.max_decoding_message_size)
            .max_encoding_message_size(self.conf.max_encoding_message_size);
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
