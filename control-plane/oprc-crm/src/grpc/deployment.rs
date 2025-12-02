use crate::crd::class_runtime::ClassRuntime;
use crate::grpc::builders::class_runtime::ClassRuntimeBuilder;
use crate::grpc::helpers::*;
use async_trait::async_trait;
use kube::Client;
use kube::Resource;
use kube::ResourceExt;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use oprc_grpc::proto::deployment::*;
use tonic::{Request, Response, Status};
use tracing::{Instrument, debug, info_span, instrument, trace, warn};

pub struct DeploymentSvc {
    pub client: Client,
    pub default_namespace: String,
}

#[async_trait]
impl oprc_grpc::proto::deployment::deployment_service_server::DeploymentService
    for DeploymentSvc
{
    #[instrument(level="debug", skip(self, request), fields(corr = request.metadata().get("x-correlation-id").and_then(|v| v.to_str().ok())))]
    async fn deploy(
        &self,
        request: Request<DeployRequest>,
    ) -> Result<Response<DeployResponse>, Status> {
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
        trace!(?deployment_unit, name, "deploy: received request");
        validate_name(&name)?;

        let api: Api<ClassRuntime> =
            Api::namespaced(self.client.clone(), &self.default_namespace);

        // Build ClassRuntime CRD from deployment unit
        let dr = ClassRuntimeBuilder::new(
            name.clone(),
            deployment_unit.id.clone(),
            corr.clone(),
            deployment_unit.clone(),
        )
        .build();

        let pp = PostParams::default();

        // Check if ClassRuntime already exists
        let existing = async {
            if let Some(d) = timeout {
                match tokio::time::timeout(d, api.get_opt(&name)).await {
                    Ok(r) => r.map_err(internal),
                    Err(_) => {
                        Err(Status::deadline_exceeded("deadline exceeded"))
                    }
                }
            } else {
                api.get_opt(&name).await.map_err(internal)
            }
        }
        .instrument(info_span!("k8s_get_classruntime", name = %name))
        .await?;

        match existing {
            Some(existing) => {
                trace!(name, "deploy: existing CR found");
                validate_existing_id(&existing, &deployment_unit.id)?
            }
            None => {
                debug!(name, "deploy: creating new ClassRuntime");
                // Create new ClassRuntime in Kubernetes
                async {
                    if let Some(d) = timeout {
                        match tokio::time::timeout(d, api.create(&pp, &dr))
                            .await
                        {
                            Ok(r) => {
                                let _ = r.map_err(internal)?;
                                Ok::<_, Status>(())
                            }
                            Err(_) => Err(Status::deadline_exceeded(
                                "deadline exceeded",
                            )),
                        }
                    } else {
                        let _ = api.create(&pp, &dr).await.map_err(internal)?;
                        Ok(())
                    }
                }
                .instrument(info_span!("k8s_create_classruntime", name = %name))
                .await?;
            }
        }

        let resp = Response::new(DeployResponse {
            status: oprc_grpc::proto::common::StatusCode::Ok as i32,
            deployment_id: deployment_unit.id,
            message: Some("accepted".into()),
        });
        Ok(attach_corr(resp, &corr))
    }

    #[instrument(level="debug", skip(self, request), fields(corr = request.metadata().get("x-correlation-id").and_then(|v| v.to_str().ok())))]
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
        let api: Api<ClassRuntime> =
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
                trace!(name, "get_deployment_status: found");
                let summary = dr
                    .status
                    .as_ref()
                    .map(|s| summarize_status(s))
                    .unwrap_or_else(|| "found".to_string());
                let deployment = Some(map_crd_to_proto(&dr));
                let status_resource_refs = dr.status.as_ref().and_then(|s| s.resource_refs.as_ref()).map(|refs| {
                    refs.iter().map(|r| oprc_grpc::proto::deployment::ResourceReference {
                        kind: r.kind.clone(),
                        name: r.name.clone(),
                        namespace: Some(dr.namespace().unwrap_or_default()),
                        uid: dr.meta().uid.clone(),
                    }).collect()
                }).unwrap_or_default();
                let resp = Response::new(GetDeploymentStatusResponse {
                    status: oprc_grpc::proto::common::StatusCode::Ok as i32,
                    deployment,
                    message: Some(summary),
                    status_resource_refs,
                });
                Ok(attach_corr(resp, &_corr))
            }
            None => {
                trace!(name, "get_deployment_status: not found");
                Err(Status::not_found("deployment not found"))
            }
        }
    }

    #[instrument(level="debug", skip(self, request), fields(corr = request.metadata().get("x-correlation-id").and_then(|v| v.to_str().ok())))]
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
        let api: Api<ClassRuntime> =
            Api::namespaced(self.client.clone(), &self.default_namespace);
        let dp = DeleteParams::default();

        // Delete ClassRuntime from Kubernetes
        let res = async {
            if let Some(d) = timeout {
                match tokio::time::timeout(d, api.delete(&name, &dp)).await {
                    Ok(r) => r,
                    Err(_) => {
                        Err(kube::Error::Api(kube::error::ErrorResponse {
                            status: "Timeout".into(),
                            message: "deadline exceeded".into(),
                            reason: "DeadlineExceeded".into(),
                            code: 504,
                        }))
                    }
                }
            } else {
                api.delete(&name, &dp).await
            }
        }
        .instrument(info_span!("k8s_delete_classruntime", name = %name))
        .await;

        match res {
            Ok(_) => {
                debug!(name, "delete_deployment: deleted");
                let resp = Response::new(DeleteDeploymentResponse {
                    status: oprc_grpc::proto::common::StatusCode::Ok as i32,
                    message: Some("deleted".into()),
                });
                Ok(attach_corr(resp, &_corr))
            }
            Err(kube::Error::Api(ref ae)) if ae.code == 504 => {
                Err(Status::deadline_exceeded("deadline exceeded"))
            }
            Err(kube::Error::Api(ae)) if ae.code == 404 => {
                trace!(name, "delete_deployment: not found");
                Err(Status::not_found("deployment not found"))
            }
            Err(e) => {
                warn!(name, error=?e, "delete_deployment: internal error");
                Err(internal(e))
            }
        }
    }

    #[instrument(level = "debug", skip(self, request))]
    async fn list_class_runtimes(
        &self,
        request: Request<ListClassRuntimesRequest>,
    ) -> Result<Response<ListClassRuntimesResponse>, Status> {
        let _corr = request
            .metadata()
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let timeout = parse_grpc_timeout(request.metadata());

        let req = request.into_inner();

        let api: Api<ClassRuntime> =
            Api::namespaced(self.client.clone(), &self.default_namespace);

        let lp = ListParams::default();
        let listed = if let Some(d) = timeout {
            match tokio::time::timeout(d, api.list(&lp)).await {
                Ok(r) => r.map_err(internal)?,
                Err(_) => {
                    return Err(Status::deadline_exceeded("deadline exceeded"));
                }
            }
        } else {
            api.list(&lp).await.map_err(internal)?
        };

        let deployments: Vec<_> = listed
            .items
            .into_iter()
            .filter(|dr| {
                if let Some(ref want) = req.status {
                    if let Some(st) = dr.status.as_ref() {
                        let s = summarize_status(st);
                        return s == *want;
                    }
                    return false;
                }
                true
            })
            .map(|dr| map_crd_to_summary(&dr))
            .collect();

        let offset = req.offset.unwrap_or(0) as usize;
        let limit = req.limit.unwrap_or(u32::MAX) as usize;
        let slice = if offset >= deployments.len() {
            &[][..]
        } else {
            let end = (offset + limit).min(deployments.len());
            &deployments[offset..end]
        };

        trace!(
            returned = slice.len(),
            total = deployments.len(),
            offset,
            limit,
            "list_class_runtimes: sliced"
        );
        let resp = Response::new(ListClassRuntimesResponse {
            items: slice.to_vec(),
        });
        Ok(resp)
    }

    #[instrument(level = "debug", skip(self, request))]
    async fn get_class_runtime(
        &self,
        request: Request<GetClassRuntimeRequest>,
    ) -> Result<Response<GetClassRuntimeResponse>, Status> {
        let req = request.into_inner();
        if req.deployment_id.is_empty() {
            return Err(Status::invalid_argument("deployment_id required"));
        }

        let name = sanitize_name(&req.deployment_id);
        validate_name(&name)?;
        let api: Api<ClassRuntime> =
            Api::namespaced(self.client.clone(), &self.default_namespace);

        match api.get_opt(&name).await.map_err(internal)? {
            Some(dr) => {
                trace!(name, "get_class_runtime: found");
                let deployment = Some(map_crd_to_summary(&dr));
                Ok(Response::new(GetClassRuntimeResponse { deployment }))
            }
            None => {
                trace!(name, "get_class_runtime: not found");
                Err(Status::not_found("deployment not found"))
            }
        }
    }
}
