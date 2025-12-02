use oprc_grpc::data_service_server::DataService;
use oprc_grpc::oprc_function_server::OprcFunction;
use oprc_grpc::{
    EmptyResponse, InvocationRequest, InvocationResponse, ListObjectsRequest,
    ObjData, ObjMeta, ObjectInvocationRequest, ObjectMetaEnvelope,
    ObjectResponse, SetKeyRequest, SetObjectRequest, SingleKeyRequest,
    SingleObjectRequest, StatsRequest, StatsResponse, ValueResponse,
};
use tonic::{Request, Response, Status};

use oprc_invoke::proxy::ObjectProxy;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::Stream;

pub struct InvocationHandler {
    proxy: ObjectProxy,
    timeout: Duration,
}

impl InvocationHandler {
    pub fn new(proxy: ObjectProxy, timeout: Duration) -> Self {
        Self { proxy, timeout }
    }
}

#[tonic::async_trait]
impl OprcFunction for InvocationHandler {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        let req = request.into_inner();
        match tokio::time::timeout(
            self.timeout,
            self.proxy.invoke_fn_with_req(&req),
        )
        .await
        {
            Ok(Ok(resp)) => Ok(Response::new(resp)),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(Status::deadline_exceeded("timeout")),
        }
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let req = request.into_inner();
        match tokio::time::timeout(
            self.timeout,
            self.proxy.invoke_obj_with_req(&req),
        )
        .await
        {
            Ok(Ok(resp)) => Ok(Response::new(resp)),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(Status::deadline_exceeded("timeout")),
        }
    }
}

pub struct DataServiceHandler {
    proxy: ObjectProxy,
    timeout: Duration,
}

impl DataServiceHandler {
    pub fn new(proxy: ObjectProxy, timeout: Duration) -> Self {
        Self { proxy, timeout }
    }
}

#[tonic::async_trait]
impl DataService for DataServiceHandler {
    async fn get(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> Result<tonic::Response<ObjectResponse>, tonic::Status> {
        let req = request.into_inner();
        let meta = ObjMeta {
            cls_id: req.cls_id,
            partition_id: req.partition_id,
            object_id: req.object_id,
        };
        match tokio::time::timeout(self.timeout, self.proxy.get_obj(&meta))
            .await
        {
            Ok(Ok(Some(obj))) => {
                Ok(tonic::Response::new(ObjectResponse { obj: Some(obj) }))
            }
            Ok(Ok(None)) => Err(tonic::Status::not_found("object not found")),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(tonic::Status::deadline_exceeded("timeout")),
        }
    }

    async fn delete(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        let req = request.into_inner();
        let meta = ObjMeta {
            cls_id: req.cls_id,
            partition_id: req.partition_id,
            object_id: req.object_id,
        };
        match tokio::time::timeout(self.timeout, self.proxy.del_obj(&meta))
            .await
        {
            Ok(Ok(())) => Ok(tonic::Response::new(EmptyResponse {})),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(tonic::Status::deadline_exceeded("timeout")),
        }
    }

    async fn set(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        let req = request.into_inner();
        let mut obj: ObjData = req.object.unwrap_or_default();
        obj.metadata = Some(ObjMeta {
            cls_id: req.cls_id,
            partition_id: req.partition_id as u32,
            object_id: req.object_id,
        });
        match tokio::time::timeout(self.timeout, self.proxy.set_obj(obj)).await
        {
            Ok(Ok(_)) => Ok(tonic::Response::new(EmptyResponse {})),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(tonic::Status::deadline_exceeded("timeout")),
        }
    }

    async fn merge(
        &self,
        _request: tonic::Request<SetObjectRequest>,
    ) -> Result<tonic::Response<ObjectResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "merge not supported by gateway",
        ))
    }

    async fn get_value(
        &self,
        _request: tonic::Request<SingleKeyRequest>,
    ) -> Result<tonic::Response<ValueResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "get_value not supported by gateway",
        ))
    }

    async fn set_value(
        &self,
        _request: tonic::Request<SetKeyRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "set_value not supported by gateway",
        ))
    }

    async fn stats(
        &self,
        _request: tonic::Request<StatsRequest>,
    ) -> Result<tonic::Response<StatsResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "stats not supported by gateway",
        ))
    }

    async fn capabilities(
        &self,
        _request: tonic::Request<oprc_grpc::CapabilitiesRequest>,
    ) -> Result<tonic::Response<oprc_grpc::CapabilitiesResponse>, tonic::Status>
    {
        // Gateway does not compute capabilities; proxy reports only routing layer knowledge.
        let resp = oprc_grpc::CapabilitiesResponse {
            string_ids: true,        // path parsing & pass-through supported
            string_entry_keys: true, // request/response pass-through supported
            granular_entry_storage: false,
            event_pipeline_v2: false, // gateway does not emit events; advertised by ODGM
        };
        Ok(tonic::Response::new(resp))
    }

    // Phase A: Granular storage RPCs - not supported by gateway (direct to ODGM)

    type ListValuesStream = tonic::codec::Streaming<oprc_grpc::ValueEnvelope>;
    type ListObjectsStream =
        Pin<Box<dyn Stream<Item = Result<ObjectMetaEnvelope, Status>> + Send>>;

    async fn delete_value(
        &self,
        _request: tonic::Request<SingleKeyRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "granular storage APIs not supported by gateway (use direct ODGM access)",
        ))
    }

    async fn batch_set_values(
        &self,
        _request: tonic::Request<oprc_grpc::BatchSetValuesRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "granular storage APIs not supported by gateway (use direct ODGM access)",
        ))
    }

    async fn list_values(
        &self,
        _request: tonic::Request<oprc_grpc::ListValuesRequest>,
    ) -> Result<tonic::Response<Self::ListValuesStream>, tonic::Status> {
        Err(tonic::Status::unimplemented(
            "granular storage APIs not supported by gateway (use direct ODGM access)",
        ))
    }

    async fn list_objects(
        &self,
        request: tonic::Request<ListObjectsRequest>,
    ) -> Result<tonic::Response<Self::ListObjectsStream>, tonic::Status> {
        let req = request.into_inner();
        // Forward to ODGM via Zenoh proxy
        match tokio::time::timeout(
            self.timeout,
            self.proxy.list_objects(
                &req.cls_id,
                req.partition_id as u16,
                req.object_id_prefix.as_deref(),
                if req.limit > 0 { Some(req.limit) } else { None },
                req.cursor,
            ),
        )
        .await
        {
            Ok(Ok(envelopes)) => {
                // Convert Vec<ObjectMetaEnvelope> to a stream
                let stream = tokio_stream::iter(
                    envelopes.into_iter().map(Ok::<_, Status>),
                );
                Ok(tonic::Response::new(
                    Box::pin(stream) as Self::ListObjectsStream
                ))
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(tonic::Status::deadline_exceeded("timeout")),
        }
    }
}
