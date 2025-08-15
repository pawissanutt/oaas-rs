use async_trait::async_trait;
use std::{net::SocketAddr, time::Duration};
use tonic::service::Routes;
use tonic::{
    Request, Response, Status, metadata::MetadataValue, transport::Server,
};
use tonic_reflection::server::Builder as ReflectionBuilder;
use tracing::info;

use kube::ResourceExt;
use kube::{
    Client, Resource,
    api::{Api, DeleteParams, PostParams},
};
use oprc_grpc::proto::deployment::deployment_service_server::{
    DeploymentService, DeploymentServiceServer,
};
use oprc_grpc::proto::deployment::*;
use oprc_grpc::proto::health::{health_service_server::HealthService, *};
use oprc_grpc::proto::package::package_service_server::PackageServiceServer as TonicPackageServer;
use oprc_grpc::server::{PackageServiceHandler, PackageServiceServer};

use crate::crd::deployment_record::{
    ConditionStatus, ConditionType, DeploymentRecord, DeploymentRecordSpec,
    DeploymentRecordStatus,
};

pub async fn run_grpc_server(
    addr: SocketAddr,
    client: Client,
    default_namespace: String,
) -> anyhow::Result<()> {
    info!("CRM gRPC listening on {}", addr);

    let reflection = ReflectionBuilder::configure().build_v1().ok();

    let health = HealthSvc::default();

    let pkg_service = PackageServiceServer::new(PackageSvcStub);
    let tonic_pkg = TonicPackageServer::new(pkg_service);

    let deploy_svc = DeploymentSvc {
        client,
        default_namespace,
    };
    let tonic_deploy = DeploymentServiceServer::new(deploy_svc);

    let mut builder = Server::builder()
        .add_service(oprc_grpc::proto::health::health_service_server::HealthServiceServer::new(
            health,
        ))
        .add_service(tonic_pkg)
        .add_service(tonic_deploy);

    if let Some(reflection) = reflection {
        builder = builder.add_service(reflection);
    }

    builder.serve(addr).await?;
    Ok(())
}

/// Build a tonic Routes service with all CRM gRPC services, suitable for embedding in axum.
pub fn build_grpc_routes(client: Client, default_namespace: String) -> Routes {
    let reflection = ReflectionBuilder::configure().build_v1().ok();

    let health = HealthSvc::default();

    let pkg_service = PackageServiceServer::new(PackageSvcStub);
    let tonic_pkg = TonicPackageServer::new(pkg_service);

    let deploy_svc = DeploymentSvc {
        client,
        default_namespace,
    };
    let tonic_deploy = DeploymentServiceServer::new(deploy_svc);

    let mut routes = Routes::new(oprc_grpc::proto::health::health_service_server::HealthServiceServer::new(
        health,
    ));
    routes = routes.add_service(tonic_pkg).add_service(tonic_deploy);

    if let Some(refl) = reflection {
        routes = routes.add_service(refl);
    }

    routes
}

#[derive(Default)]
struct HealthSvc;

#[async_trait]
impl HealthService for HealthSvc {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse { status: 1 })) // SERVING
    }

    type WatchStream = tokio_stream::wrappers::ReceiverStream<
        Result<HealthCheckResponse, Status>,
    >;

    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

struct PackageSvcStub;

#[async_trait]
impl PackageServiceHandler for PackageSvcStub {
    async fn create_package(
        &self,
        _request: Request<oprc_grpc::proto::package::CreatePackageRequest>,
    ) -> Result<
        Response<oprc_grpc::proto::package::CreatePackageResponse>,
        Status,
    > {
        Err(Status::unimplemented(
            "create_package not implemented in CRM",
        ))
    }

    async fn get_package(
        &self,
        _request: Request<oprc_grpc::proto::package::GetPackageRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::GetPackageResponse>, Status>
    {
        Err(Status::unimplemented("get_package not implemented in CRM"))
    }

    async fn list_packages(
        &self,
        _request: Request<oprc_grpc::proto::package::ListPackagesRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::ListPackagesResponse>, Status>
    {
        Err(Status::unimplemented(
            "list_packages not implemented in CRM",
        ))
    }

    async fn delete_package(
        &self,
        _request: Request<oprc_grpc::proto::package::DeletePackageRequest>,
    ) -> Result<
        Response<oprc_grpc::proto::package::DeletePackageResponse>,
        Status,
    > {
        Err(Status::unimplemented(
            "delete_package not implemented in CRM",
        ))
    }

    async fn deploy_class(
        &self,
        _request: Request<oprc_grpc::proto::package::DeployClassRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::DeployClassResponse>, Status>
    {
        Err(Status::unimplemented("deploy_class not implemented in CRM"))
    }

    async fn report_deployment_status(
        &self,
        _request: Request<oprc_grpc::proto::package::ReportStatusRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::ReportStatusResponse>, Status>
    {
        Err(Status::unimplemented(
            "report_deployment_status not implemented in CRM",
        ))
    }
}

struct DeploymentSvc {
    client: Client,
    default_namespace: String,
}

#[async_trait]
impl DeploymentService for DeploymentSvc {
    async fn deploy(
        &self,
        request: Request<DeployRequest>,
    ) -> Result<Response<DeployResponse>, Status> {
        // Extract correlation id for observability and CRD annotations
        let corr = request
            .metadata()
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let timeout = parse_grpc_timeout(request.metadata());

        let req = request.into_inner();
        let Some(deployment_unit) = req.deployment_unit else {
            return Err(Status::invalid_argument(
                "deployment_unit is required",
            ));
        };
        let name = sanitize_name(&deployment_unit.id);
        validate_name(&name)?;
        let api: Api<DeploymentRecord> =
            Api::namespaced(self.client.clone(), &self.default_namespace);
        let dr =
            build_deployment_record(&name, &deployment_unit.id, corr.clone());

        let pp = PostParams::default();
        let existing = if let Some(d) = timeout {
            match tokio::time::timeout(d, api.get_opt(&name)).await {
                Ok(r) => r.map_err(internal)?,
                Err(_) => {
                    return Err(Status::deadline_exceeded("deadline exceeded"));
                }
            }
        } else {
            api.get_opt(&name).await.map_err(internal)?
        };
        match existing {
            Some(existing) => {
                // already exists -> idempotent accept
                validate_existing_id(&existing, &deployment_unit.id)?;
            }
            None => {
                if let Some(d) = timeout {
                    match tokio::time::timeout(d, api.create(&pp, &dr)).await {
                        Ok(r) => {
                            let _ = r.map_err(internal)?;
                        }
                        Err(_) => {
                            return Err(Status::deadline_exceeded(
                                "deadline exceeded",
                            ));
                        }
                    }
                } else {
                    let _ = api.create(&pp, &dr).await.map_err(internal)?;
                }
            }
        }

        let resp = Response::new(DeployResponse {
            status: oprc_grpc::proto::common::StatusCode::Ok as i32,
            deployment_id: deployment_unit.id,
            message: Some("accepted".into()),
        });
        Ok(attach_corr(resp, &corr))
    }

    async fn get_deployment_status(
        &self,
        request: Request<GetDeploymentStatusRequest>,
    ) -> Result<Response<GetDeploymentStatusResponse>, Status> {
        let _corr = request
            .metadata()
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let timeout = parse_grpc_timeout(request.metadata());

        let req = request.into_inner();
        if req.deployment_id.is_empty() {
            return Err(Status::invalid_argument("deployment_id required"));
        }
        let name = sanitize_name(&req.deployment_id);
        validate_name(&name)?;
        let api: Api<DeploymentRecord> =
            Api::namespaced(self.client.clone(), &self.default_namespace);
        let found = if let Some(d) = timeout {
            match tokio::time::timeout(d, api.get_opt(&name)).await {
                Ok(r) => r.map_err(internal)?,
                Err(_) => {
                    return Err(Status::deadline_exceeded("deadline exceeded"));
                }
            }
        } else {
            api.get_opt(&name).await.map_err(internal)?
        };
        match found {
            Some(dr) => {
                let summary = dr
                    .status
                    .as_ref()
                    .map(|s| summarize_status(s))
                    .unwrap_or_else(|| "found".to_string());
                let deployment = Some(map_crd_to_proto(&dr));
                let resp = Response::new(GetDeploymentStatusResponse {
                    status: oprc_grpc::proto::common::StatusCode::Ok as i32,
                    deployment,
                    message: Some(summary),
                });
                Ok(attach_corr(resp, &_corr))
            }
            None => Err(Status::not_found("deployment not found")),
        }
    }

    async fn delete_deployment(
        &self,
        request: Request<DeleteDeploymentRequest>,
    ) -> Result<Response<DeleteDeploymentResponse>, Status> {
        let _corr = request
            .metadata()
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let timeout = parse_grpc_timeout(request.metadata());

        let req = request.into_inner();
        if req.deployment_id.is_empty() {
            return Err(Status::invalid_argument("deployment_id required"));
        }
        let name = sanitize_name(&req.deployment_id);
        validate_name(&name)?;
        let api: Api<DeploymentRecord> =
            Api::namespaced(self.client.clone(), &self.default_namespace);
        let dp = DeleteParams::default();
        let res = if let Some(d) = timeout {
            match tokio::time::timeout(d, api.delete(&name, &dp)).await {
                Ok(r) => r,
                Err(_) => {
                    return Err(Status::deadline_exceeded("deadline exceeded"));
                }
            }
        } else {
            api.delete(&name, &dp).await
        };
        match res {
            Ok(_) => {
                let resp = Response::new(DeleteDeploymentResponse {
                    status: oprc_grpc::proto::common::StatusCode::Ok as i32,
                    message: Some("deleted".into()),
                });
                Ok(attach_corr(resp, &_corr))
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                Err(Status::not_found("deployment not found"))
            }
            Err(e) => Err(internal(e)),
        }
    }
}

#[allow(dead_code)]
fn sanitize_name(id: &str) -> String {
    let mut s = id.to_ascii_lowercase();
    s = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    s.trim_matches('-').to_string()
}

#[allow(dead_code)]
fn internal<E: std::fmt::Display>(e: E) -> Status {
    Status::internal(e.to_string())
}

fn attach_corr<T>(mut resp: Response<T>, corr: &Option<String>) -> Response<T> {
    if let Some(c) = corr {
        if let Ok(v) = MetadataValue::try_from(c.clone()) {
            resp.metadata_mut().insert("x-correlation-id", v);
        }
    }
    resp
}

const LABEL_DEPLOYMENT_ID: &str = "oaas.io/deployment-id";
const ANNO_CORRELATION_ID: &str = "oaas.io/correlation-id";

fn build_deployment_record(
    name: &str,
    deployment_id: &str,
    corr: Option<String>,
) -> DeploymentRecord {
    let mut dr = DeploymentRecord::new(
        name,
        DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            function: None,
            nfr_requirements: None,
            nfr: None,
        },
    );
    let labels = dr.meta_mut().labels.get_or_insert_with(Default::default);
    labels.insert(LABEL_DEPLOYMENT_ID.into(), deployment_id.to_string());
    if let Some(c) = corr {
        let ann = dr
            .meta_mut()
            .annotations
            .get_or_insert_with(Default::default);
        ann.insert(ANNO_CORRELATION_ID.into(), c);
    }
    dr
}

fn summarize_status(s: &DeploymentRecordStatus) -> String {
    if let Some(conds) = &s.conditions {
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Available)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return "Available".to_string();
        }
        if let Some(c) = conds.iter().find(|c| {
            matches!(c.type_, ConditionType::Degraded)
                && matches!(c.status, ConditionStatus::True)
        }) {
            let mut msg = "Degraded".to_string();
            if let Some(r) = &c.reason {
                msg.push_str(&format!(" ({})", r));
            }
            if let Some(m) = &c.message {
                msg.push_str(&format!(": {}", m));
            }
            return msg;
        }
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Progressing)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return "Progressing".to_string();
        }
    }
    if let Some(p) = &s.phase {
        return p.clone();
    }
    if let Some(m) = &s.message {
        return m.clone();
    }
    "found".to_string()
}

fn map_crd_to_proto(
    dr: &DeploymentRecord,
) -> oprc_grpc::proto::deployment::OClassDeployment {
    use oprc_grpc::proto::{
        common as oaas_common, deployment as oaas_deployment,
    };

    let key = dr
        .metadata
        .labels
        .as_ref()
        .and_then(|m| m.get(LABEL_DEPLOYMENT_ID))
        .cloned()
        .unwrap_or_else(|| dr.name_any());

    // created_at: map from k8s metadata if available
    let created_at = dr.metadata.creation_timestamp.as_ref().and_then(|t| {
        // k8s-openapi Time often wraps chrono::DateTime<Utc> in .0
        #[allow(deprecated)]
        let ts_secs = t.0.timestamp();
        #[allow(deprecated)]
        let ts_nanos = t.0.timestamp_subsec_nanos() as i32;
        Some(oaas_common::Timestamp {
            seconds: ts_secs,
            nanos: ts_nanos,
        })
    });

    oaas_deployment::OClassDeployment {
        key,
        package_name: String::new(),
        class_key: String::new(),
        target_env: String::new(),
        target_clusters: vec![],
        nfr_requirements: None,
        functions: vec![],
        created_at,
    }
}

fn parse_grpc_timeout(meta: &tonic::metadata::MetadataMap) -> Option<Duration> {
    let val = meta.get("grpc-timeout")?.to_str().ok()?;
    if val.is_empty() {
        return None;
    }
    let (num_part, unit_part) = val.split_at(val.len().saturating_sub(1));
    let n: u64 = num_part.parse().ok()?;
    match unit_part {
        "H" => Some(Duration::from_secs(n.saturating_mul(3600))),
        "M" => Some(Duration::from_secs(n.saturating_mul(60))),
        "S" => Some(Duration::from_secs(n)),
        "m" => Some(Duration::from_millis(n)),
        "u" => Some(Duration::from_micros(n)),
        "n" => Some(Duration::from_nanos(n)),
        _ => None,
    }
}

fn validate_name(name: &str) -> Result<(), Status> {
    if name.is_empty() {
        return Err(Status::invalid_argument(
            "invalid deployment_id: empty after sanitization",
        ));
    }
    if name.len() > 253 {
        return Err(Status::invalid_argument(
            "invalid deployment_id: length exceeds 253 characters",
        ));
    }
    Ok(())
}

fn validate_existing_id(
    existing: &DeploymentRecord,
    requested_id: &str,
) -> Result<(), Status> {
    let labels = existing.metadata.labels.as_ref();
    let Some(lbls) = labels else {
        return Err(Status::already_exists(
            "resource name in use by a different deployment (no label)",
        ));
    };
    match lbls.get(LABEL_DEPLOYMENT_ID) {
        Some(v) if v == requested_id => Ok(()),
        _ => Err(Status::already_exists(
            "resource name in use by a different deployment",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::deployment_record::{
        Condition, ConditionStatus, ConditionType, DeploymentRecordStatus,
    };
    use oprc_grpc::proto::health::HealthCheckRequest;

    #[test]
    fn test_sanitize_name_cleans_invalid_chars() {
        let input = "My_Deployment.ID@Prod";
        let out = sanitize_name(input);
        assert_eq!(out, "my-deployment-id-prod");
    }

    #[test]
    fn test_build_deployment_record_sets_labels_and_corr() {
        let dr = build_deployment_record(
            "unit-1",
            "deploy-123",
            Some("corr-abc".to_string()),
        );
        let meta = dr.metadata;
        let labels = meta.labels.unwrap();
        assert_eq!(labels.get(LABEL_DEPLOYMENT_ID).unwrap(), "deploy-123");
        let ann = meta.annotations.unwrap();
        assert_eq!(ann.get(ANNO_CORRELATION_ID).unwrap(), "corr-abc");
    }

    #[tokio::test]
    async fn test_health_check_serving() {
        let svc = HealthSvc::default();
        let req =
            tonic::Request::new(HealthCheckRequest { service: "".into() });
        let resp = svc.check(req).await.unwrap().into_inner();
        assert_eq!(resp.status, 1);
    }

    #[test]
    fn test_attach_corr_sets_response_metadata() {
        let corr = Some("cid-123".to_string());
        let resp = Response::new(42u32);
        let resp = attach_corr(resp, &corr);
        let mv = resp.metadata().get("x-correlation-id").unwrap();
        assert_eq!(mv.to_str().unwrap(), "cid-123");
    }

    #[test]
    fn test_summarize_status_prefers_available_then_progressing() {
        let s = DeploymentRecordStatus {
            phase: Some("Progressing".into()),
            message: None,
            observed_generation: None,
            last_updated: None,
            conditions: Some(vec![
                Condition {
                    type_: ConditionType::Progressing,
                    status: ConditionStatus::True,
                    reason: None,
                    message: None,
                    last_transition_time: None,
                },
                Condition {
                    type_: ConditionType::Available,
                    status: ConditionStatus::True,
                    reason: None,
                    message: None,
                    last_transition_time: None,
                },
            ]),
            resource_refs: None,
            nfr_recommendations: None,
            last_applied_recommendations: None,
            last_applied_at: None,
        };
        assert_eq!(summarize_status(&s), "Available");

        let s2 = DeploymentRecordStatus {
            conditions: Some(vec![Condition {
                type_: ConditionType::Progressing,
                status: ConditionStatus::True,
                reason: None,
                message: None,
                last_transition_time: None,
            }]),
            ..Default::default()
        };
        assert_eq!(summarize_status(&s2), "Progressing");
    }

    #[test]
    fn test_summarize_status_degraded_includes_reason_message() {
        let s = DeploymentRecordStatus {
            conditions: Some(vec![Condition {
                type_: ConditionType::Degraded,
                status: ConditionStatus::True,
                reason: Some("CrashLoop".into()),
                message: Some("pod restarting".into()),
                last_transition_time: None,
            }]),
            ..Default::default()
        };
        let summary = summarize_status(&s);
        assert!(summary.starts_with("Degraded (CrashLoop): pod restarting"));
    }

    #[test]
    fn test_validate_name_rejects_empty_and_long() {
        assert!(validate_name("").is_err());
        let long = "a".repeat(254);
        assert!(validate_name(&long).is_err());
        assert!(validate_name("ok-name").is_ok());
    }

    #[test]
    fn test_parse_grpc_timeout_various_units() {
        use tonic::metadata::MetadataMap;
        let mut md = MetadataMap::new();
        md.insert("grpc-timeout", "10S".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), Some(Duration::from_secs(10)));
        md.insert("grpc-timeout", "500m".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), Some(Duration::from_millis(500)));
        md.insert("grpc-timeout", "1H".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), Some(Duration::from_secs(3600)));
        md.insert("grpc-timeout", "3M".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), Some(Duration::from_secs(180)));
        md.insert("grpc-timeout", "250u".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), Some(Duration::from_micros(250)));
        md.insert("grpc-timeout", "100n".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), Some(Duration::from_nanos(100)));
        md.insert("grpc-timeout", "abc".parse().unwrap());
        assert_eq!(parse_grpc_timeout(&md), None);
    }

    #[test]
    fn test_validate_existing_id_ok_and_conflict() {
        let mut dr = build_deployment_record("n", "id-1", None);
        assert!(validate_existing_id(&dr, "id-1").is_ok());
        dr.metadata
            .labels
            .as_mut()
            .unwrap()
            .insert(LABEL_DEPLOYMENT_ID.into(), "id-2".into());
        assert!(matches!(
            validate_existing_id(&dr, "id-1"),
            Err(Status { .. })
        ));
        dr.metadata.labels = None;
        assert!(validate_existing_id(&dr, "id-1").is_err());
    }

    #[test]
    fn test_map_crd_to_proto_uses_label_and_defaults() {
        let dr = build_deployment_record("name-x", "deploy-abc", None);
        let mapped = map_crd_to_proto(&dr);
        assert_eq!(mapped.key, "deploy-abc");
        assert!(mapped.created_at.is_none());
        assert!(mapped.functions.is_empty());
        assert!(mapped.target_clusters.is_empty());
    }
}
