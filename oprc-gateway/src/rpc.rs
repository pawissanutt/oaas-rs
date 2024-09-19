use http::uri::InvalidUri;
use mobc::Manager;
use oprc_proto::oprc_function_client::OprcFunctionClient;
use std::str::FromStr;
use tonic::transport::Channel;
use tonic::transport::Uri;

#[derive(Debug)]
pub struct RpcManager {
    uri: Uri,
}

impl RpcManager {
    pub fn new(addr: &str) -> Result<Self, InvalidUri> {
        let uri = Uri::from_str(addr)?;
        Ok(Self { uri })
    }
}

#[async_trait::async_trait]
impl Manager for RpcManager {
    type Connection = OprcFunctionClient<Channel>;

    type Error = InvalidUri;

    async fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let ch = Channel::builder(self.uri.clone()).connect_lazy();
        let client = OprcFunctionClient::new(ch);
        Ok(client)
    }

    async fn check(&self, conn: Self::Connection) -> Result<Self::Connection, Self::Error> {
        Ok(conn)
    }
}
