use std::{
    collections::HashMap,
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use envconfig::Envconfig;
use oprc_dev::{Config, FuncReq, generate_partition_id};
use oprc_offload::proxy::ObjectProxy;
use oprc_pb::{
    InvocationRequest, InvocationResponse, ObjData, ObjMeta,
    ObjectInvocationRequest, ResponseStatus,
    oprc_function_server::{OprcFunction, OprcFunctionServer},
    val_data::Data,
};
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
    info!("use {:?}", z);
    let z = z.create_zenoh();
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
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
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
        let obj = oprc_dev::rand_obj(
            &func_req,
            ObjMeta {
                cls_id: req.cls_id,
                partition_id: self.partition_id,
                object_id: rand::random::<u64>(),
            },
        )
        .map_err(|e| {
            error!("failed to create obj: {:?}", e);
            tonic::Status::internal(e.to_string())
        })?;
        info!("func_req: {:?}", func_req);

        let resp = if func_req.resp_json {
            let val = obj.entries.get(&0).unwrap();
            let out_payload = match &val.data {
                Some(Data::Byte(b)) => b.clone(),
                _ => vec![],
            };

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
        let obj = oprc_dev::rand_obj(
            &func_req,
            ObjMeta {
                cls_id: req.cls_id,
                partition_id: req.partition_id,
                object_id: req.object_id,
            },
        )
        .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let resp = if func_req.resp_json {
            let val = obj.entries.get(&0).unwrap();
            let out_payload = match &val.data {
                Some(Data::Byte(b)) => b.clone(),
                _ => vec![],
            };
            self.update_obj(obj).await?;
            InvocationResponse {
                payload: Some(out_payload),
                status: ResponseStatus::Okay as i32,
                headers: HashMap::from([("env".to_string(), self.env.clone())]),
                ..Default::default()
            }
        } else {
            self.update_obj(obj).await?;
            InvocationResponse {
                payload: None,
                status: ResponseStatus::Okay as i32,
                headers: HashMap::from([("env".to_string(), self.env.clone())]),
                ..Default::default()
            }
        };
        Ok(Response::new(resp))
    }
}
