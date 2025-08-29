use crate::{
    config::CrmClientConfig,
    errors::CrmError,
    models::{
        ClusterHealth, DeploymentRecord, DeploymentRecordFilter,
        DeploymentResponse, DeploymentStatus,
    },
};
use chrono::TimeZone;
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
        // Prefer CRM-specific info RPC which includes node counts
        use oprc_grpc::proto::health::CrmClusterRequest;
        use oprc_grpc::proto::health::crm_info_service_client::CrmInfoServiceClient;

        let mut client = CrmInfoServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

        let resp = client
            .get_cluster_health(CrmClusterRequest {
                cluster: String::new(),
            })
            .await;

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
                    cluster_name: info.cluster_name,
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
                use oprc_grpc::proto::health::health_service_client::HealthServiceClient;
                use oprc_grpc::proto::health::{
                    HealthCheckRequest, health_check_response,
                };

                let mut hclient = HealthServiceClient::connect(
                    self.endpoint.clone(),
                )
                .await
                .map_err(|e| CrmError::ConfigurationError(e.to_string()))?;

                let hresp = hclient
                    .check(HealthCheckRequest {
                        service: String::new(),
                    })
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
                .map(|f| {
                    let provision_config =
                        f.provision_config.as_ref().map(|pc| {
                            grpc_types::ProvisionConfig {
                                container_image: pc.container_image.clone(),
                                port: pc.port.map(|p| p as u32),
                                max_concurrency: pc.max_concurrency,
                                need_http2: pc.need_http2,
                                cpu_request: pc.cpu_request.clone(),
                                memory_request: pc.memory_request.clone(),
                                cpu_limit: pc.cpu_limit.clone(),
                                memory_limit: pc.memory_limit.clone(),
                                min_scale: pc.min_scale,
                                max_scale: pc.max_scale,
                            }
                        });

                    // Use DU-level NFR as a default per-function until models are updated
                    let nfr = &unit.nfr_requirements;
                    grpc_types::FunctionDeploymentSpec {
                        function_key: f.function_key.clone(),
                        description: f.description.clone(),
                        available_location: f.available_location.clone(),
                        nfr_requirements: Some(grpc_types::NfrRequirements {
                            max_latency_ms: nfr.max_latency_ms,
                            min_throughput_rps: nfr.min_throughput_rps,
                            availability: nfr.availability,
                            cpu_utilization_target: nfr.cpu_utilization_target,
                        }),
                        provision_config,
                        config: f.config.clone(),
                    }
                })
                .collect(),
            target_env: unit.target_env.clone(),
            created_at: Some(grpc_types::Timestamp {
                seconds: unit.created_at.timestamp(),
                nanos: unit.created_at.timestamp_subsec_nanos() as i32,
            }),
            odgm_config: unit.odgm.as_ref().map(|o| grpc_types::OdgmConfig {
                collections: o.collections.clone(),
                partition_count: Some(o.partition_count as u32),
                replica_count: Some(o.replica_count as u32),
                shard_type: Some(o.shard_type.clone()),
                invocations: None,
                options: std::collections::HashMap::new(),
            }),
        };

        match client.deploy(du).await {
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
        let resource_refs = status_resp
            .status_resource_refs
            .iter()
            .cloned()
            .map(|r| crate::models::ResourceReference {
                kind: r.kind,
                name: r.name,
                namespace: r.namespace,
                uid: r.uid,
            })
            .collect::<Vec<_>>();

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
            resource_refs,
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

            // DeploymentUnit doesn't carry summarized_status; default to Pending
            let condition = DeploymentCondition::Pending;
            let message = None;

            // No embedded resource refs on items when using DeploymentUnit
            let resource_refs: Vec<crate::models::ResourceReference> =
                Vec::new();

            items.push(DeploymentRecord {
                id: d.id.clone(),
                deployment_unit_id: d.id.clone(),
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
                resource_refs,
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
