use crate::crd::deployment_record::DeploymentRecord;
use crate::grpc::builders::deployment_record::DeploymentRecordBuilder;
use crate::grpc::helpers::*;
use async_trait::async_trait;
use kube::Client;
use kube::Resource;
use kube::ResourceExt;
use kube::api::{Api, DeleteParams, ListParams, PostParams};
use oprc_grpc::proto::deployment::*;
use tonic::{Request, Response, Status};

pub struct DeploymentSvc {
    pub client: Client,
    pub default_namespace: String,
}

#[async_trait]
impl oprc_grpc::proto::deployment::deployment_service_server::DeploymentService
    for DeploymentSvc
{
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
        validate_name(&name)?;
        let api: Api<DeploymentRecord> =
            Api::namespaced(self.client.clone(), &self.default_namespace);
        let dr = DeploymentRecordBuilder::new(
            name.clone(),
            deployment_unit.id.clone(),
            corr.clone(),
            deployment_unit.clone(),
        )
        .build();

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
                validate_existing_id(&existing, &deployment_unit.id)?
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

    async fn list_deployment_records(
        &self,
        request: Request<ListDeploymentRecordsRequest>,
    ) -> Result<Response<ListDeploymentRecordsResponse>, Status> {
        let _corr = request
            .metadata()
            .get("x-correlation-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let timeout = parse_grpc_timeout(request.metadata());

        let req = request.into_inner();

        let api: Api<DeploymentRecord> =
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
            .map(|dr| map_crd_to_proto(&dr))
            .collect();

        let offset = req.offset.unwrap_or(0) as usize;
        let limit = req.limit.unwrap_or(u32::MAX) as usize;
        let slice = if offset >= deployments.len() {
            &[][..]
        } else {
            let end = (offset + limit).min(deployments.len());
            &deployments[offset..end]
        };

        let resp = Response::new(ListDeploymentRecordsResponse {
            items: slice.to_vec(),
        });
        Ok(resp)
    }

    async fn get_deployment_record(
        &self,
        request: Request<GetDeploymentRecordRequest>,
    ) -> Result<Response<GetDeploymentRecordResponse>, Status> {
        let req = request.into_inner();
        if req.deployment_id.is_empty() {
            return Err(Status::invalid_argument("deployment_id required"));
        }

        let name = sanitize_name(&req.deployment_id);
        validate_name(&name)?;
        let api: Api<DeploymentRecord> =
            Api::namespaced(self.client.clone(), &self.default_namespace);

        match api.get_opt(&name).await.map_err(internal)? {
            Some(dr) => {
                let deployment = Some(map_crd_to_proto(&dr));
                Ok(Response::new(GetDeploymentRecordResponse { deployment }))
            }
            None => Err(Status::not_found("deployment not found")),
        }
    }
}
