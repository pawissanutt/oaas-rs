use std::net::SocketAddr;
use tokio::{net::TcpListener, sync::oneshot};
use tonic::{transport::Server, Request, Response, Status};

use oprc_pb::{
    oprc_function_server::{OprcFunction, OprcFunctionServer},
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
    ResponseStatus,
};

#[allow(dead_code)]
pub struct MockFnHandle {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

#[allow(dead_code)]
impl MockFnHandle {
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

#[derive(Default)]
pub struct MockFnService;

#[tonic::async_trait]
impl OprcFunction for MockFnService {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let req = request.into_inner();
        // Echo back the payload, always OKAY
        Ok(Response::new(InvocationResponse {
            payload: Some(req.payload),
            status: ResponseStatus::Okay as i32,
            headers: Default::default(),
            invocation_id: "mock-invocation-id".to_string(),
        }))
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let req = request.into_inner();
        // Echo back the payload, always OKAY
        Ok(Response::new(InvocationResponse {
            payload: Some(req.payload),
            status: ResponseStatus::Okay as i32,
            headers: Default::default(),
            invocation_id: "mock-invocation-id".to_string(),
        }))
    }
}

#[allow(dead_code)]
/// Start a mock gRPC function server on a random port.
/// Returns a handle with the bound address and shutdown control.
pub async fn start_mock_fn_server() -> MockFnHandle {
    let svc = OprcFunctionServer::new(MockFnService::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(svc)
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async {
                    let _ = rx.await;
                },
            )
            .await
            .ok();
    });

    MockFnHandle {
        addr,
        shutdown: Some(tx),
        handle: Some(handle),
    }
}
