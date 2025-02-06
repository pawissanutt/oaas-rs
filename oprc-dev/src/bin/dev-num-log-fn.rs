use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant, UNIX_EPOCH},
};

use envconfig::Envconfig;
use oprc_dev::{
    num_log::{LoggingReq, LoggingResp, Mode},
    Config,
};
use oprc_offload::proxy::ObjectProxy;
use oprc_pb::{
    oprc_function_server::{OprcFunction, OprcFunctionServer},
    val_data::Data,
    InvocationRequest, InvocationResponse, ObjData, ObjMeta,
    ObjectInvocationRequest, ResponseStatus, ValData,
};
use tokio::signal;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{debug, error, info};
fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { start().await.unwrap() });
}

async fn start() -> Result<(), Box<dyn Error + Send + Sync>> {
    tracing_subscriber::fmt::init();
    let conf = Config::init_from_env()?;
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);
    let z = oprc_zenoh::OprcZenohConfig::init_from_env()?;
    info!("use {:?}", z);
    let z = z.create_zenoh();
    let z_session = zenoh::open(z).await?;
    let proxy = ObjectProxy::new(z_session);

    let random_fn = LoggingFunction { proxy };
    let echo_function: OprcFunctionServer<LoggingFunction> =
        OprcFunctionServer::new(random_fn);
    tracing::info!("start server on port {}", conf.http_port);
    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();
    Server::builder()
        .add_service(echo_function.max_decoding_message_size(usize::MAX))
        .add_service(reflection_server_v1a)
        .add_service(reflection_server_v1)
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
        tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct JsonState {
    num: u64,
}
struct LoggingFunction {
    proxy: ObjectProxy,
}

impl LoggingFunction {
    async fn handle_read(
        &self,
        obj_req: &ObjectInvocationRequest,
        log_req: &LoggingReq,
    ) -> Result<LoggingResp, tonic::Status> {
        debug!(
            "handle_read started with duration {} ms and inteval {} ms",
            log_req.duration, log_req.inteval
        );
        let start = Instant::now();
        let mut log = Vec::new();
        let mut last_num = 0;

        while start.elapsed().as_millis() < log_req.duration as u128 {
            debug!(
                "Iteration started, elapsed {} ms",
                start.elapsed().as_millis()
            );
            let start_i = Instant::now();
            let obj =
                self.proxy.get_obj(&ObjMeta::from(obj_req)).await.map_err(
                    |e| {
                        error!("failed to get obj: {:?}", e);
                        tonic::Status::internal(e.to_string())
                    },
                )?;
            if let Some(val) = obj.entries.get(&0) {
                if let Some(Data::Byte(b)) = &val.data {
                    let s: JsonState = serde_json::from_slice(b).unwrap();
                    let ts = std::time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    debug!(
                        "Read object state: num = {} at timestamp {}",
                        s.num, ts
                    );
                    log.push((s.num, ts));
                    last_num = s.num;
                }
            }
            let sleep_time =
                log_req.inteval - start_i.elapsed().as_millis() as u64;
            if sleep_time > 0 {
                tokio::time::sleep(Duration::from_millis(sleep_time)).await;
            }
        }
        let ts = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Ok(LoggingResp {
            log,
            num: last_num,
            ts,
        })
    }

    async fn handle_write(
        &self,
        obj_req: &ObjectInvocationRequest,
        log_req: &LoggingReq,
    ) -> Result<LoggingResp, tonic::Status> {
        info!("handle_write: setting object with num = {}", log_req.num);
        let mut obj = ObjData {
            metadata: Some(ObjMeta::from(obj_req)),
            ..Default::default()
        };
        let s = JsonState { num: log_req.num };
        let state_vec = serde_json::to_vec(&s).unwrap();
        obj.entries.insert(
            0,
            ValData {
                data: Some(Data::Byte(state_vec)),
            },
        );
        self.proxy.set_obj(obj).await.map_err(|e| {
            error!("failed to create obj: {:?}", e);
            tonic::Status::internal(e.to_string())
        })?;
        let ts = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        debug!("Object successfully written with num = {}", log_req.num);
        Ok(LoggingResp {
            num: log_req.num,
            ts,
            ..Default::default()
        })
    }
}

#[tonic::async_trait]
impl OprcFunction for LoggingFunction {
    async fn invoke_fn(
        &self,
        _request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        Err(Status::unimplemented("not implemented"))
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let obj_req = request.into_inner();
        let req = LoggingReq::try_from(&obj_req)?;
        info!("invoke_obj received with mode: {:?}", req.mode);
        debug!("req: {:?} {:?}", obj_req, req);
        let resp = match req.mode {
            Mode::READ => self.handle_read(&obj_req, &req).await,
            Mode::WRITE => self.handle_write(&obj_req, &req).await,
        };
        resp.map(|r| {
            let payload = serde_json::to_vec(&r).unwrap();
            Response::new(InvocationResponse {
                payload: Some(payload),
                status: ResponseStatus::Okay as i32,
            })
        })
    }
}
