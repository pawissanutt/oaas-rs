use std::{
    collections::HashMap,
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use envconfig::Envconfig;
use oprc_dev::{Config, FuncReq, generate_partition_id, rand_json};
use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjData, ObjMeta,
    ObjectInvocationRequest, ResponseStatus, ValData, ValType,
    oprc_function_server::{OprcFunction, OprcFunctionServer},
};
use oprc_invoke::proxy::ObjectProxy;
use tokio::signal;
use tonic::{Request, Response, Status, transport::Server};
use tracing::{debug, error, info};

fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);
    tracing_subscriber::fmt::init();

    info!(
        "Starting tokio runtime with {} worker threads",
        worker_threads
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { start().await.unwrap() });
}

async fn start() -> Result<(), Box<dyn Error + Send + Sync>> {
    let conf = Config::init_from_env()?;
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);
    let z = oprc_zenoh::OprcZenohConfig::init_from_env()?;
    let z = z.create_zenoh();
    info!("use {:?}", z);
    let z_session = zenoh::open(z).await?;
    let proxy = ObjectProxy::new(z_session);
    let partition_id = generate_partition_id();

    let random_fn = RandomFunction {
        proxy,
        partition_id,
        env: conf.env.clone(),
        env_id: conf.env_id,
    };
    let random_function: OprcFunctionServer<RandomFunction> =
        OprcFunctionServer::new(random_fn);
    tracing::info!("start server on port {}", conf.http_port);
    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_grpc::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_grpc::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();
    Server::builder()
        .add_service(random_function.max_decoding_message_size(usize::MAX))
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

struct RandomFunction {
    proxy: ObjectProxy,
    partition_id: u32,
    env: String,
    env_id: u32,
}

impl RandomFunction {
    async fn update_obj(&self, obj: ObjData) -> Result<(), tonic::Status> {
        let t = std::time::Instant::now();
        let meta = obj.metadata.clone();
        self.proxy.set_obj(obj).await.map_err(|e| {
            error!("failed to create obj: {:?}", e);
            tonic::Status::internal(e.to_string())
        })?;
        info!(
            "{:?} write in {} ms",
            meta.unwrap(),
            t.elapsed().as_millis()
        );
        Ok(())
    }
}

#[tonic::async_trait]
impl OprcFunction for RandomFunction {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        info!("test");
        let req = request.into_inner();
        debug!("req: {:?} {}", req, String::from_utf8_lossy(&req.payload));
        let func_req = FuncReq::try_from(&req)?;

        // Build an object that uses string entry keys (entries_str) with one JSON payload under key "0"
        let mut entries_str = HashMap::new();
        let out_payload_bytes = rand_json(&func_req)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        entries_str.insert(
            "0".to_string(),
            ValData {
                data: out_payload_bytes.clone().into(),
                r#type: ValType::Byte as i32,
            },
        );
        let obj = ObjData {
            metadata: Some(ObjMeta {
                cls_id: req.cls_id,
                partition_id: self.partition_id,
                object_id: rand::random::<u64>(),
                object_id_str: None,
            }),
            entries: Default::default(),
            event: None,
            entries_str,
        };
        info!("func_req: {:?}", func_req);

        let resp = if func_req.resp_json {
            let val = obj
                .entries_str
                .get("0")
                .ok_or_else(|| Status::internal("missing key '0'"))?;
            let out_payload = val.data.clone();

            self.update_obj(obj).await?;
            InvocationResponse {
                payload: Some(out_payload),
                status: ResponseStatus::Okay as i32,
                headers: HashMap::from([
                    ("env".to_string(), self.env.clone()),
                    ("env_id".to_string(), self.env_id.to_string()),
                ]),
                ..Default::default()
            }
        } else {
            self.update_obj(obj).await?;
            InvocationResponse {
                payload: None,
                status: ResponseStatus::Okay as i32,
                headers: HashMap::from([
                    ("env".to_string(), self.env.clone()),
                    ("env_id".to_string(), self.env_id.to_string()),
                ]),
                ..Default::default()
            }
        };

        Ok(Response::new(resp))
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let req = request.into_inner();
        debug!("req: {:?} {}", req, String::from_utf8_lossy(&req.payload));
        let func_req = FuncReq::try_from(&req)?;
        info!(
            "pid: {}, oid: {}, func_req: {:?}",
            req.partition_id, req.object_id, func_req
        );

        // Build object using string entry keys for the provided object id
        let mut entries_str = HashMap::new();
        let out_payload_bytes = rand_json(&func_req)
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        entries_str.insert(
            "0".to_string(),
            ValData {
                data: out_payload_bytes.clone().into(),
                r#type: ValType::Byte as i32,
            },
        );
        let obj = ObjData {
            metadata: Some(ObjMeta {
                cls_id: req.cls_id,
                partition_id: req.partition_id,
                object_id: req.object_id,
                object_id_str: None,
            }),
            entries: Default::default(),
            event: None,
            entries_str,
        };

        let resp = if func_req.resp_json {
            let val = obj
                .entries_str
                .get("0")
                .ok_or_else(|| Status::internal("missing key '0'"))?;
            let out_payload = val.data.clone();
            self.update_obj(obj).await?;
            InvocationResponse {
                payload: Some(out_payload),
                status: ResponseStatus::Okay as i32,
                headers: HashMap::from([
                    ("env".to_string(), self.env.clone()),
                    ("env_id".to_string(), self.env_id.to_string()),
                ]),
                ..Default::default()
            }
        } else {
            self.update_obj(obj).await?;
            InvocationResponse {
                payload: None,
                status: ResponseStatus::Okay as i32,
                headers: HashMap::from([
                    ("env".to_string(), self.env.clone()),
                    ("env_id".to_string(), self.env_id.to_string()),
                ]),
                ..Default::default()
            }
        };
        Ok(Response::new(resp))
    }
}
