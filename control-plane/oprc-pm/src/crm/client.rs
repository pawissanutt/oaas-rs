use crate::{
    config::CrmClientConfig,
    errors::CrmError,
    models::{
        ClusterHealth, DeploymentRecord, DeploymentRecordFilter,
        DeploymentResponse, DeploymentStatus,
    },
};
use oprc_grpc::client::deployment_client::DeploymentClient as GrpcDeploymentClient;
use oprc_grpc::types as grpc_types;
use oprc_models::{DeploymentCondition, DeploymentUnit};
use tokio::sync::Mutex;
use tokio::sync::MutexGuard;
use tracing::info;

pub struct CrmClient {
    endpoint: String,
    cluster_name: String,
    #[allow(unused)]
    config: CrmClientConfig,
    // gRPC deployment client (lazily connected; tonic client requires &mut self)
    grpc_deploy: Mutex<Option<GrpcDeploymentClient>>,
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
            config,
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

    fn map_status_code_to_condition(&self, code: i32) -> DeploymentCondition {
        use oprc_grpc::proto::common::StatusCode;
        match code {
            x if x == (StatusCode::Ok as i32) => DeploymentCondition::Running,
            x if x == (StatusCode::NotFound as i32) => {
                DeploymentCondition::Down
            }
            x if x == (StatusCode::InvalidRequest as i32) => {
                DeploymentCondition::Down
            }
            x if x == (StatusCode::Error as i32)
                || x == (StatusCode::InternalError as i32) =>
            {
                DeploymentCondition::Down
            }
            _ => DeploymentCondition::Pending,
        }
    }

    pub async fn health_check(&self) -> Result<ClusterHealth, CrmError> {
        info!("Performing health check for cluster: {}", self.cluster_name);

        // Use gRPC Health service
        use oprc_grpc::proto::health::health_service_client::HealthServiceClient;
        use oprc_grpc::proto::health::{
            HealthCheckRequest, health_check_response,
        };

        let mut client = HealthServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

        let resp = client
            .check(HealthCheckRequest {
                service: String::new(),
            })
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?
            .into_inner();

        let status_str = match resp.status {
            x if x
                == (health_check_response::ServingStatus::Serving as i32) =>
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
        })
    }

    pub async fn deploy(
        &self,
        unit: DeploymentUnit,
    ) -> Result<DeploymentResponse, CrmError> {
        info!(
            "Deploying unit {} to cluster: {}",
            unit.id, self.cluster_name
        );

        // gRPC path using persistent client
        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard
            .as_mut()
            .expect("gRPC client must be initialized");

        let du = grpc_types::DeploymentUnit {
            id: unit.id.clone(),
            package_name: unit.package_name.clone(),
            class_key: unit.class_key.clone(),
            target_cluster: unit.target_cluster.clone(),
            functions: unit
                .functions
                .iter()
                .map(|f| grpc_types::FunctionDeploymentSpec {
                    function_key: f.function_key.clone(),
                    replicas: f.replicas,
                    resource_requirements: Some(
                        grpc_types::ResourceRequirements {
                            cpu_request: f
                                .resource_requirements
                                .cpu_request
                                .clone(),
                            memory_request: f
                                .resource_requirements
                                .memory_request
                                .clone(),
                            cpu_limit: f
                                .resource_requirements
                                .cpu_limit
                                .clone(),
                            memory_limit: f
                                .resource_requirements
                                .memory_limit
                                .clone(),
                        },
                    ),
                })
                .collect(),
            target_env: unit.target_env.clone(),
            nfr_requirements: Some(grpc_types::NfrRequirements {
                max_latency_ms: unit.nfr_requirements.max_latency_ms,
                min_throughput_rps: unit.nfr_requirements.min_throughput_rps,
                availability: unit.nfr_requirements.availability,
                cpu_utilization_target: unit
                    .nfr_requirements
                    .cpu_utilization_target,
            }),
            created_at: Some(grpc_types::Timestamp {
                seconds: unit.created_at.timestamp(),
                nanos: unit.created_at.timestamp_subsec_nanos() as i32,
            }),
        };

        match client.deploy(du).await {
            Ok(resp) => Ok(DeploymentResponse {
                id: resp.deployment_id,
                status: format!("{:?}", resp.status),
                message: resp.message,
            }),
            Err(status) => {
                Err(CrmError::ConfigurationError(status.to_string()))
            }
        }
    }

    pub async fn get_deployment_status(
        &self,
        id: &str,
    ) -> Result<DeploymentStatus, CrmError> {
        info!(
            "Getting deployment status for {} from cluster: {}",
            id, self.cluster_name
        );

        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard
            .as_mut()
            .expect("gRPC client must be initialized");
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

    pub async fn get_deployment_record(
        &self,
        id: &str,
    ) -> Result<DeploymentRecord, CrmError> {
        info!(
            "Getting deployment record for {} from cluster: {}",
            id, self.cluster_name
        );

        // Enrich using both record and status when available
        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard
            .as_mut()
            .expect("gRPC client must be initialized");

        let status_resp = client
            .get_deployment_status(id.to_string())
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

        let condition = self.map_status_code_to_condition(status_resp.status);
        let message = status_resp.message.clone();

        // Attempt to use the optional deployment payload to fill metadata
        let (package_name, class_key, target_env, created_at) =
            if let Some(dep) = status_resp.deployment {
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
                    dep.package_name,
                    dep.class_key,
                    dep.target_env,
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

        Ok(DeploymentRecord {
            id: id.to_string(),
            deployment_unit_id: id.to_string(),
            package_name,
            class_key,
            target_environment: target_env,
            cluster_name: Some(self.cluster_name.clone()),
            status: crate::models::DeploymentRecordStatus {
                condition,
                phase: crate::models::DeploymentPhase::Unknown,
                message,
                last_updated: chrono::Utc::now().to_rfc3339(),
            },
            nfr_compliance: None,
            resource_refs: vec![],
            created_at: created_at.clone(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    pub async fn list_deployment_records(
        &self,
        filter: DeploymentRecordFilter,
    ) -> Result<Vec<DeploymentRecord>, CrmError> {
        info!(
            "Listing deployment records from cluster: {} with filter: {:?}",
            self.cluster_name, filter
        );

        // gRPC call
        let mut guard = self.ensure_deploy_client().await?;
        let client = guard.as_mut().expect("gRPC client must be initialized");

        let req = oprc_grpc::proto::deployment::ListDeploymentRecordsRequest {
            package_name: filter.package_name.clone(),
            class_key: filter.class_key.clone(),
            target_env: filter.environment.clone(),
            status: filter.status.clone(),
            limit: filter.limit.map(|v| v as u32),
            offset: filter.offset.map(|v| v as u32),
        };

        let resp = client
            .list_deployment_records(req)
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

        let mut items = Vec::with_capacity(resp.items.len());
        for d in resp.items.into_iter() {
            // Map created_at if available
            let created_at = d
                .created_at
                .as_ref()
                .and_then(|t| {
                    chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32)
                })
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();

            // Try to fetch status per record (best-effort)
            let status_resp =
                client.get_deployment_status(d.key.clone()).await.ok();
            let (condition, message) = if let Some(sr) = status_resp {
                (self.map_status_code_to_condition(sr.status), sr.message)
            } else {
                (DeploymentCondition::Pending, None)
            };

            items.push(DeploymentRecord {
                id: d.key.clone(),
                deployment_unit_id: d.key.clone(),
                package_name: d.package_name,
                class_key: d.class_key,
                target_environment: d.target_env,
                cluster_name: Some(self.cluster_name.clone()),
                status: crate::models::DeploymentRecordStatus {
                    condition,
                    phase: crate::models::DeploymentPhase::Unknown,
                    message,
                    last_updated: chrono::Utc::now().to_rfc3339(),
                },
                nfr_compliance: None,
                resource_refs: vec![],
                created_at,
                updated_at: chrono::Utc::now().to_rfc3339(),
            });
        }

        Ok(items)
    }

    pub async fn delete_deployment(&self, id: &str) -> Result<(), CrmError> {
        info!(
            "Deleting deployment {} from cluster: {}",
            id, self.cluster_name
        );

        let mut client_guard = self.ensure_deploy_client().await?;
        let client = client_guard
            .as_mut()
            .expect("gRPC client must be initialized");
        client
            .delete_deployment(id.to_string())
            .await
            .map_err(|status| {
                CrmError::ConfigurationError(status.to_string())
            })?;
        Ok(())
    }
}
