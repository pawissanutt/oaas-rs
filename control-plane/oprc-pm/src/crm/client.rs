use crate::{
    config::CrmClientConfig,
    errors::CrmError,
    models::{
        ClassRuntime, ClassRuntimeFilter, ClusterHealth, DeploymentResponse,
        DeploymentStatus,
    },
};
use chrono::TimeZone;
use oprc_grpc::client::deployment_client::DeploymentClient as GrpcDeploymentClient;
use oprc_grpc::client::topology_client::TopologyClient as GrpcTopologyClient;
use oprc_grpc::proto::health::crm_info_service_client::CrmInfoServiceClient;
use oprc_grpc::proto::health::health_service_client::HealthServiceClient;
use oprc_grpc::proto::health::{
    CrmEnvRequest, HealthCheckRequest, health_check_response,
};
use oprc_grpc::proto::runtime as runtime_proto;
use oprc_grpc::proto::topology::TopologySnapshot;
use oprc_grpc::tracing::inject_trace_context;
use oprc_grpc::types as grpc_types;
use oprc_grpc::{Channel, Request};
use tokio::sync::Mutex;
use tokio::sync::MutexGuard;
use tracing::info;

pub struct CrmClient {
    endpoint: String,
    cluster_name: String,
    #[allow(unused)]
    config: CrmClientConfig,
    // gRPC clients (lazily connected; tonic client requires &mut self)
    grpc_deploy: Mutex<Option<GrpcDeploymentClient>>,
    grpc_topology: Mutex<Option<GrpcTopologyClient>>,
    grpc_crm_info: Mutex<Option<CrmInfoServiceClient<Channel>>>,
}

impl CrmClient {
    pub fn new(
        cluster_name: String,
        config: CrmClientConfig,
    ) -> Result<Self, CrmError> {
        // gRPC requires an http(s) endpoint for tonic
        if !(config.url.starts_with("http://")
            || config.url.starts_with("https://"))
        {
            return Err(CrmError::ConfigurationError(
                "CRM URL must start with http:// or https:// for gRPC".into(),
            ));
        }

        Ok(Self {
            endpoint: config.url.clone(),
            cluster_name,
            grpc_deploy: Mutex::new(None),
            grpc_topology: Mutex::new(None),
            grpc_crm_info: Mutex::new(None),
            config,
        })
    }

    #[inline]
    pub fn retry_attempts(&self) -> u32 {
        self.config.retry_attempts.max(1)
    }

    #[inline]
    pub fn timeout_duration(&self) -> Option<std::time::Duration> {
        self.config.timeout.and_then(|s| {
            if s == 0 {
                None
            } else {
                Some(std::time::Duration::from_secs(s))
            }
        })
    }

    async fn ensure_deploy_client(
        &self,
    ) -> Result<MutexGuard<'_, Option<GrpcDeploymentClient>>, CrmError> {
        let mut guard = self.grpc_deploy.lock().await;
        if guard.is_none() {
            let client = GrpcDeploymentClient::connect(self.endpoint.clone())
                .await
                .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;
            *guard = Some(client);
        }
        Ok(guard)
    }

    async fn ensure_topology_client(
        &self,
    ) -> Result<MutexGuard<'_, Option<GrpcTopologyClient>>, CrmError> {
        let mut guard = self.grpc_topology.lock().await;
        if guard.is_none() {
            let client = GrpcTopologyClient::connect(self.endpoint.clone())
                .await
                .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;
            *guard = Some(client);
        }
        Ok(guard)
    }

    async fn ensure_crm_info_client(
        &self,
    ) -> Result<MutexGuard<'_, Option<CrmInfoServiceClient<Channel>>>, CrmError>
    {
        let mut guard = self.grpc_crm_info.lock().await;
        if guard.is_none() {
            let client = CrmInfoServiceClient::connect(self.endpoint.clone())
                .await
                .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;
            *guard = Some(client);
        }
        Ok(guard)
    }

    fn map_status_code_to_condition(
        &self,
        code: i32,
    ) -> runtime_proto::DeploymentCondition {
        use oprc_grpc::proto::common::StatusCode;
        match code {
            x if x == (StatusCode::Ok as i32) => {
                runtime_proto::DeploymentCondition::Running
            }
            x if x == (StatusCode::NotFound as i32) => {
                runtime_proto::DeploymentCondition::Down
            }
            x if x == (StatusCode::InvalidRequest as i32) => {
                runtime_proto::DeploymentCondition::Down
            }
            x if x == (StatusCode::Error as i32)
                || x == (StatusCode::InternalError as i32) =>
            {
                runtime_proto::DeploymentCondition::Down
            }
            _ => runtime_proto::DeploymentCondition::Pending,
        }
    }

    #[tracing::instrument(skip(self), fields(cluster = %self.cluster_name))]
    pub async fn health_check(&self) -> Result<ClusterHealth, CrmError> {
        info!("Performing health check for cluster: {}", self.cluster_name);

        // Use cached CRM info client
        let mut guard = self.ensure_crm_info_client().await?;
        let client = guard.as_mut().ok_or_else(|| {
            CrmError::InternalError("CRM info client not initialized".into())
        })?;

        let mut request = Request::new(CrmEnvRequest { env: String::new() });
        inject_trace_context(&mut request);
        let resp = client.get_env_health(request).await;

        match resp {
            Ok(r) => {
                let info = r.into_inner();
                let last_seen = if let Some(t) = info.last_seen {
                    // Prefer the newer timestamp helper to avoid deprecated APIs
                    chrono::Utc
                        .timestamp_opt(t.seconds, t.nanos as u32)
                        .single()
                        .unwrap_or_else(|| chrono::Utc::now())
                } else {
                    chrono::Utc::now()
                };
                Ok(ClusterHealth {
                    cluster_name: info.env_name,
                    status: info.status,
                    crm_version: info.crm_version,
                    last_seen,
                    node_count: info.node_count,
                    ready_nodes: info.ready_nodes,
                    availability: info.availability,
                })
            }
            Err(e) => {
                // Fall back to lightweight HealthService if CRM info not available
                tracing::warn!(
                    "CrmInfoService unavailable, falling back to HealthService: {}",
                    e
                );
                let mut hclient = HealthServiceClient::connect(
                    self.endpoint.clone(),
                )
                .await
                .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

                let mut hrequest = Request::new(HealthCheckRequest {
                    service: String::new(),
                });
                inject_trace_context(&mut hrequest);
                let hresp = hclient
                    .check(hrequest)
                    .await
                    .map_err(|e| CrmError::ConfigurationError(e.to_string()))?
                    .into_inner();

                let status_str = match hresp.status {
                    x if x
                        == (health_check_response::ServingStatus::Serving
                            as i32) =>
                    {
                        "Healthy"
                    }
                    x if x
                        == (health_check_response::ServingStatus::NotServing
                            as i32) =>
                    {
                        "Unhealthy"
                    }
                    _ => "Unknown",
                };

                Ok(ClusterHealth {
                    cluster_name: self.cluster_name.clone(),
                    status: status_str.to_string(),
                    crm_version: None,
                    last_seen: chrono::Utc::now(),
                    node_count: None,
                    ready_nodes: None,
                    availability: None,
                })
            }
        }
    }

    #[tracing::instrument(skip(self, unit), fields(cluster = %self.cluster_name, unit_id = %unit.id))]
    pub async fn deploy(
        &self,
        unit: grpc_types::DeploymentUnit,
    ) -> Result<DeploymentResponse, CrmError> {
        info!(
            "Deploying unit {} to cluster: {}",
            unit.id, self.cluster_name
        );

        // Log the outgoing payload for troubleshooting schema/type issues
        match serde_json::to_string(&unit) {
            Ok(json) => {
                tracing::debug!(target="oprc_pm::crm::client", cluster=%self.cluster_name, id=%unit.id, payload=%json, "Outgoing DeploymentUnit payload");
            }
            Err(e) => {
                tracing::warn!(target="oprc_pm::crm::client", cluster=%self.cluster_name, id=%unit.id, error=%e, "Failed to serialize DeploymentUnit for debug log");
            }
        }

        // gRPC path using persistent client
        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard.as_mut().ok_or_else(|| {
            CrmError::InternalError("Deploy client not initialized".into())
        })?;

        match client.deploy(unit).await {
            Ok(resp) => Ok(DeploymentResponse {
                id: resp.deployment_id,
                status: format!(
                    "{:?}",
                    self.map_status_code_to_condition(resp.status)
                ),
                message: resp.message,
            }),
            Err(status) => {
                Err(CrmError::ConfigurationError(status.to_string()))
            }
        }
    }

    #[tracing::instrument(skip(self), fields(cluster = %self.cluster_name))]
    pub async fn get_deployment_status(
        &self,
        id: &str,
    ) -> Result<DeploymentStatus, CrmError> {
        info!(
            "Getting deployment status for {} from cluster: {}",
            id, self.cluster_name
        );

        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard.as_mut().ok_or_else(|| {
            CrmError::InternalError("Deploy client not initialized".into())
        })?;
        match client.get_deployment_status(id.to_string()).await {
            Ok(resp) => Ok(DeploymentStatus {
                id: id.to_string(),
                status: format!(
                    "{:?}",
                    self.map_status_code_to_condition(resp.status)
                ),
                phase: "Unknown".to_string(),
                message: resp.message,
                last_updated: chrono::Utc::now().to_rfc3339(),
            }),
            Err(status) => {
                Err(CrmError::ConfigurationError(status.to_string()))
            }
        }
    }

    #[tracing::instrument(skip(self), fields(cluster = %self.cluster_name))]
    pub async fn get_class_runtime(
        &self,
        id: &str,
    ) -> Result<ClassRuntime, CrmError> {
        info!(
            "Getting class runtime for {} from cluster: {}",
            id, self.cluster_name
        );

        // Enrich using both record and status when available
        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard.as_mut().ok_or_else(|| {
            CrmError::InternalError("Deploy client not initialized".into())
        })?;

        let status_resp = client
            .get_deployment_status(id.to_string())
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

        let condition = self.map_status_code_to_condition(status_resp.status);
        let message = status_resp.message.clone();

        // Attempt to use the optional deployment payload to fill metadata
        let (package_name, class_key, target_env, created_at) =
            if let Some(ref dep) = status_resp.deployment {
                let ts = dep
                    .created_at
                    .map(|t| {
                        chrono::DateTime::from_timestamp(
                            t.seconds,
                            t.nanos as u32,
                        )
                    })
                    .flatten()
                    .unwrap_or_else(chrono::Utc::now);
                (
                    dep.package_name.clone(),
                    dep.class_key.clone(),
                    dep.target_env.clone(),
                    ts.to_rfc3339(),
                )
            } else {
                (
                    "unknown".to_string(),
                    "unknown".to_string(),
                    "unknown".to_string(),
                    chrono::Utc::now().to_rfc3339(),
                )
            };

        // Resource refs not embedded on DeploymentUnit; use status-level refs only
        let resource_refs = status_resp.status_resource_refs.clone();

        Ok(ClassRuntime {
            id: id.to_string(),
            deployment_unit_id: id.to_string(),
            package_name,
            class_key,
            target_environment: target_env,
            cluster_name: Some(self.cluster_name.clone()),
            status: Some(crate::models::ClassRuntimeStatus {
                condition: condition as i32,
                phase: runtime_proto::DeploymentPhase::PhaseRunning as i32,
                message,
                last_updated: chrono::Utc::now().to_rfc3339(),
                functions: vec![],
            }),
            nfr_compliance: None,
            resource_refs,
            created_at: created_at.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    #[tracing::instrument(skip(self), fields(cluster = %self.cluster_name))]
    pub async fn list_class_runtimes(
        &self,
        filter: ClassRuntimeFilter,
    ) -> Result<Vec<ClassRuntime>, CrmError> {
        info!(
            "Listing deployment records from cluster: {} with filter: {:?}",
            self.cluster_name, filter
        );

        // gRPC call
        let mut guard = self.ensure_deploy_client().await?;
        let client = guard.as_mut().ok_or_else(|| {
            CrmError::InternalError("Deploy client not initialized".into())
        })?;

        let req = oprc_grpc::proto::deployment::ListClassRuntimesRequest {
            package_name: filter.package_name.clone(),
            class_key: filter.class_key.clone(),
            target_env: filter.environment.clone(),
            status: filter.status.clone(),
            limit: filter.limit.map(|v| v as u32),
            offset: filter.offset.map(|v| v as u32),
        };

        let resp = client
            .list_class_runtimes(req)
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

        // Items are already ClassRuntimeSummary from runtime proto; just tag cluster_name if missing.
        let items = resp
            .items
            .into_iter()
            .map(|mut s| {
                if s.cluster_name.is_none() {
                    s.cluster_name = Some(self.cluster_name.clone());
                }
                s
            })
            .collect();

        Ok(items)
    }

    #[tracing::instrument(skip(self), fields(cluster = %self.cluster_name))]
    pub async fn delete_deployment(&self, id: &str) -> Result<(), CrmError> {
        info!(
            "Deleting deployment {} from cluster: {}",
            id, self.cluster_name
        );

        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard.as_mut().ok_or_else(|| {
            CrmError::InternalError("Deploy client not initialized".into())
        })?;
        client
            .delete_deployment(id.to_string())
            .await
            .map_err(|status| {
                CrmError::ConfigurationError(status.to_string())
            })?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(cluster = %self.cluster_name))]
    pub async fn get_topology(&self) -> Result<TopologySnapshot, CrmError> {
        let mut guard = self.ensure_topology_client().await?;
        if let Some(client) = guard.as_mut() {
            client
                .get_topology(None)
                .await
                .map_err(|e| CrmError::InternalError(e.to_string()))
        } else {
            Err(CrmError::InternalError("Failed to connect to CRM".into()))
        }
    }
}
