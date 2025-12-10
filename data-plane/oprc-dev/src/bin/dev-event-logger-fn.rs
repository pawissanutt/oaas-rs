//! Event Logger Function - Demonstrates the eventing system
//!
//! This function is designed to be triggered by events from other functions.
//! It receives a TriggerPayload and logs the event information.
//!
//! Use case: Set this as an `on_complete` or `on_error` trigger target for
//! another function to observe the event chain and trace propagation.

use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use envconfig::Envconfig;
use oprc_dev::Config;
use oprc_grpc::oprc_function_server::{OprcFunction, OprcFunctionServer};
use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
    ResponseStatus, TriggerPayload,
};
use prost::Message;
use tokio::signal;
use tonic::{Request, Response, Status, transport::Server};
use tracing::{debug, info};

fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);
    tracing_subscriber::fmt::init();

    info!(
        "Starting event-logger-fn with {} worker threads",
        worker_threads
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { start().await.unwrap() });
}

async fn start() -> Result<(), Box<dyn Error>> {
    let conf = Config::init_from_env()?;
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);
    let function: OprcFunctionServer<EventLoggerFunction> =
        OprcFunctionServer::new(EventLoggerFunction {});
    
    info!("Starting event-logger server on port {}", conf.http_port);
    
    let (reflection_v1a, reflection_v1) = oprc_dev::create_reflection();
    
    Server::builder()
        .add_service(function.max_decoding_message_size(usize::MAX))
        .add_service(reflection_v1a)
        .add_service(reflection_v1)
        .serve_with_shutdown(socket, shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("signal received, starting graceful shutdown");
}

struct EventLoggerFunction;

#[tonic::async_trait]
impl OprcFunction for EventLoggerFunction {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            cls_id = %req.cls_id,
            fn_id = %req.fn_id,
            partition_id = req.partition_id,
            "Received stateless invocation (event trigger)"
        );
        
        // Try to parse the payload as TriggerPayload
        let event_info = parse_trigger_payload(&req.payload);
        
        if let Some(info) = &event_info {
            info!(
                source_cls = %info.source_cls_id,
                source_partition = info.source_partition_id,
                source_object = ?info.source_object_id,
                event_type = info.event_type,
                fn_id = ?info.fn_id,
                timestamp = info.timestamp,
                "📨 Event received from source"
            );
        }
        
        // Return success response with event summary
        let response_payload = serde_json::json!({
            "status": "event_logged",
            "event_info": event_info.map(|e| serde_json::json!({
                "source_cls_id": e.source_cls_id,
                "source_partition_id": e.source_partition_id,
                "source_object_id": e.source_object_id,
                "event_type": format_event_type(e.event_type),
                "fn_id": e.fn_id,
                "timestamp": e.timestamp,
            })),
        });
        
        Ok(Response::new(InvocationResponse {
            status: ResponseStatus::Okay as i32,
            payload: Some(serde_json::to_vec(&response_payload).unwrap_or_default()),
            ..Default::default()
        }))
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let req = request.into_inner();
        let object_id = req.object_id.clone().unwrap_or_default();
        
        info!(
            cls_id = %req.cls_id,
            fn_id = %req.fn_id,
            partition_id = req.partition_id,
            object_id = %object_id,
            "Received object invocation (event trigger)"
        );
        
        // Try to parse the payload as TriggerPayload
        let event_info = parse_trigger_payload(&req.payload);
        
        if let Some(info) = &event_info {
            info!(
                source_cls = %info.source_cls_id,
                source_partition = info.source_partition_id,
                source_object = ?info.source_object_id,
                event_type = info.event_type,
                fn_id = ?info.fn_id,
                timestamp = info.timestamp,
                "📨 Event received for object {}", object_id
            );
        }
        
        let response_payload = serde_json::json!({
            "status": "event_logged",
            "target_object_id": object_id,
            "event_info": event_info.map(|e| serde_json::json!({
                "source_cls_id": e.source_cls_id,
                "source_partition_id": e.source_partition_id,
                "source_object_id": e.source_object_id,
                "event_type": format_event_type(e.event_type),
                "fn_id": e.fn_id,
                "timestamp": e.timestamp,
            })),
        });
        
        Ok(Response::new(InvocationResponse {
            status: ResponseStatus::Okay as i32,
            payload: Some(serde_json::to_vec(&response_payload).unwrap_or_default()),
            ..Default::default()
        }))
    }
}

/// Parse TriggerPayload from bytes - tries both JSON and protobuf formats
fn parse_trigger_payload(payload: &[u8]) -> Option<EventInfoSummary> {
    // First try JSON (default format)
    if let Ok(trigger) = serde_json::from_slice::<TriggerPayload>(payload) {
        if let Some(info) = trigger.event_info {
            return Some(EventInfoSummary {
                source_cls_id: info.source_cls_id,
                source_partition_id: info.source_partition_id,
                source_object_id: info.source_object_id,
                event_type: info.event_type,
                fn_id: info.fn_id,
                timestamp: info.timestamp,
            });
        }
    }
    
    // Fall back to protobuf
    if let Ok(trigger) = TriggerPayload::decode(payload) {
        if let Some(info) = trigger.event_info {
            return Some(EventInfoSummary {
                source_cls_id: info.source_cls_id,
                source_partition_id: info.source_partition_id,
                source_object_id: info.source_object_id,
                event_type: info.event_type,
                fn_id: info.fn_id,
                timestamp: info.timestamp,
            });
        }
    }
    
    // Not a trigger payload - might be a direct invocation
    debug!("Payload is not a TriggerPayload (might be direct invocation)");
    None
}

#[derive(Debug)]
struct EventInfoSummary {
    source_cls_id: String,
    source_partition_id: u32,
    source_object_id: Option<String>,
    event_type: i32,
    fn_id: Option<String>,
    timestamp: u64,
}

fn format_event_type(event_type: i32) -> &'static str {
    match event_type {
        1 => "FUNC_COMPLETE",
        2 => "FUNC_ERROR", 
        3 => "DATA_CREATE",
        4 => "DATA_UPDATE",
        5 => "DATA_DELETE",
        _ => "UNKNOWN",
    }
}
