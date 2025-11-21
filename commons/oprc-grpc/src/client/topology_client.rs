use crate::proto::topology::*;
use tonic::transport::Channel;

#[derive(Clone)]
pub struct TopologyClient {
    client: topology_service_client::TopologyServiceClient<Channel>,
}

impl TopologyClient {
    pub async fn connect(
        endpoint: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client =
            topology_service_client::TopologyServiceClient::connect(endpoint)
                .await?;
        Ok(Self { client })
    }

    pub async fn get_topology(
        &mut self,
        source: Option<String>,
    ) -> Result<TopologySnapshot, tonic::Status> {
        let request = tonic::Request::new(TopologyRequest {
            source: source.unwrap_or_default(),
        });
        let response = self.client.get_topology(request).await?;
        Ok(response.into_inner())
    }
}
