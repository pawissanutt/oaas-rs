use crate::grpc::helpers::count_nodes;
use async_trait::async_trait;
use k8s_openapi::api::core::v1::Node;
use kube::Client;
use kube::api::{Api, ListParams};
use oprc_grpc::proto::common::Timestamp as GrpcTimestamp;
use oprc_grpc::proto::health::{CrmClusterHealth, CrmClusterRequest};
use std::sync::Arc;
use tonic::{Request, Response, Status};

// Environment variable to force a mock availability value for this CRM instance.
// Value must parse as f64 in [0,1]. If present, overrides dynamic computation.
const ENV_MOCK_AVAILABILITY: &str = "OPRC_MOCK_CLUSTER_AVAILABILITY";

#[async_trait]
pub trait NodeProvider: Send + Sync + 'static {
    async fn list_nodes(&self) -> Result<Vec<Node>, kube::Error>;
}

struct KubeNodeProvider {
    client: Client,
}

#[async_trait]
impl NodeProvider for KubeNodeProvider {
    async fn list_nodes(&self) -> Result<Vec<Node>, kube::Error> {
        let api: Api<Node> = Api::all(self.client.clone());
        let lp = ListParams::default();
        let node_list = api.list(&lp).await?;
        Ok(node_list.items)
    }
}

pub struct CrmInfoSvc {
    provider: Arc<dyn NodeProvider>,
    namespace: String,
    mock_availability: Option<f64>,
}

impl CrmInfoSvc {
    pub fn new(client: Client, namespace: String) -> Self {
        let provider = Arc::new(KubeNodeProvider { client });
        let mock_availability = std::env::var(ENV_MOCK_AVAILABILITY)
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v.clamp(0.0, 1.0));
        Self {
            provider,
            namespace,
            mock_availability,
        }
    }

    pub fn with_provider<P: NodeProvider>(
        provider: P,
        namespace: String,
    ) -> Self {
        let mock_availability = std::env::var(ENV_MOCK_AVAILABILITY)
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v.clamp(0.0, 1.0));
        Self {
            provider: Arc::new(provider),
            namespace,
            mock_availability,
        }
    }
}

#[async_trait]
impl oprc_grpc::proto::health::crm_info_service_server::CrmInfoService
    for CrmInfoSvc
{
    async fn get_cluster_health(
        &self,
        request: Request<CrmClusterRequest>,
    ) -> Result<Response<CrmClusterHealth>, Status> {
        let cluster = request.into_inner().cluster;

        let nodes = self.provider.list_nodes().await.map_err(|e| {
            Status::internal(format!("Failed to list nodes: {}", e))
        })?;

        let (node_count, ready_nodes) = count_nodes(&nodes);

        let now = chrono::Utc::now();
        let ts = GrpcTimestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        };

        let default_name = if self.namespace.is_empty() {
            "local".to_string()
        } else {
            self.namespace.clone()
        };

        let resp = CrmClusterHealth {
            cluster_name: if cluster.is_empty() {
                default_name
            } else {
                cluster
            },
            status: "Healthy".to_string(),
            crm_version: None,
            last_seen: Some(ts),
            node_count: Some(node_count),
            ready_nodes: Some(ready_nodes),
            availability: self.mock_availability.or(Some(0.99)),
        };

        Ok(Response::new(resp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Node;
    use serde_json::json;
    use tonic::Request as TonicRequest;

    struct MockProvider {
        nodes: Vec<Node>,
    }

    #[async_trait]
    impl NodeProvider for MockProvider {
        async fn list_nodes(&self) -> Result<Vec<Node>, kube::Error> {
            Ok(self.nodes.clone())
        }
    }

    use oprc_grpc::proto::health::crm_info_service_server::CrmInfoService as _CrmTrait;

    #[tokio::test]
    async fn test_get_cluster_health_counts_nodes() {
        let n1: Node = serde_json::from_value(json!({ "metadata": {}, "status": { "conditions": [{ "type": "Ready", "status": "True" }] } })).unwrap();
        let n2: Node = serde_json::from_value(json!({ "metadata": {}, "status": { "conditions": [{ "type": "Ready", "status": "False" }] } })).unwrap();

        let provider = MockProvider {
            nodes: vec![n1, n2],
        };
        let svc = CrmInfoSvc::with_provider(provider, "test-ns".to_string());

        let req = TonicRequest::new(CrmClusterRequest { cluster: "".into() });
        let resp = svc.get_cluster_health(req).await.unwrap().into_inner();
        assert_eq!(resp.node_count.unwrap(), 2);
        assert_eq!(resp.ready_nodes.unwrap(), 1);
        assert_eq!(resp.cluster_name, "test-ns");
    }

    #[tokio::test]
    async fn test_env_mock_availability_overrides() {
        // Force value via env (wrapped in unsafe due to platform/toolchain marking these functions unsafe)
        unsafe {
            std::env::set_var(super::ENV_MOCK_AVAILABILITY, "0.42");
        }
        let n1: Node = serde_json::from_value(json!({ "metadata": {}, "status": { "conditions": [{ "type": "Ready", "status": "True" }] } })).unwrap();
        let n2: Node = serde_json::from_value(json!({ "metadata": {}, "status": { "conditions": [{ "type": "Ready", "status": "True" }] } })).unwrap();
        let provider = MockProvider {
            nodes: vec![n1, n2],
        };
        let svc =
            CrmInfoSvc::with_provider(provider, "override-ns".to_string());
        let req = TonicRequest::new(CrmClusterRequest { cluster: "".into() });
        let resp = svc.get_cluster_health(req).await.unwrap().into_inner();
        assert!((resp.availability.unwrap() - 0.42).abs() < 1e-9);
        unsafe {
            std::env::remove_var(super::ENV_MOCK_AVAILABILITY);
        }
    }
}
