use std::{
    collections::HashMap,
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use dashmap::DashMap;
use envconfig::Envconfig;
use oprc_dev::create_reflection;
use oprc_pb::{
    ClsRouting, ClsRoutingRequest, ClsRoutingTable, FuncRouting,
    PartitionRouting,
    routing_service_server::{RoutingService, RoutingServiceServer},
};
use tokio::sync::watch::{self};
use tokio_stream::wrappers::WatchStream;
use tonic::{Request, Response, Status, transport::Server};

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

async fn start() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    let conf = oprc_dev::Config::init_from_env()?;
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);
    let dev_conf = DevConfig::init_from_env()?;
    let dev_pm = DevPM::new(dev_conf.get_cls_list());
    let dev_pm = RoutingServiceServer::new(dev_pm);
    tracing::info!("start server on port {}", conf.http_port);
    let (v1a, v1) = create_reflection();
    Server::builder()
        .add_service(dev_pm)
        .add_service(v1a)
        .add_service(v1)
        .serve(socket)
        .await?;
    Ok(())
}

#[derive(envconfig::Envconfig)]
struct DevConfig {
    #[envconfig(
        from = "PM_CLS_LIST",
        default = "example!echo=localhost:8080|other=localhost:8080,"
    )]
    cls_list: String,
}

impl DevConfig {
    fn get_cls_list(&self) -> DashMap<String, DashMap<String, String>> {
        let cls_list: Vec<&str> = self.cls_list.split(",").collect();
        let map: DashMap<String, DashMap<String, String>> = DashMap::new();
        for cls_expr in cls_list {
            let cls_fn: Vec<&str> = cls_expr.split("!").collect();
            let cls_name = cls_fn.get(0).unwrap();
            if cls_name.len() == 0 {
                continue;
            }
            let fn_list = cls_fn
                .get(1)
                .unwrap_or(&"")
                .split("|")
                .map(|s| String::from(s));
            let fn_map = DashMap::new();
            for fn_uri in fn_list {
                let fn_uri: Vec<&str> = fn_uri.split("=").collect();
                fn_map.insert(
                    String::from(*fn_uri.get(0).unwrap()),
                    String::from(*fn_uri.get(1).unwrap()),
                );
            }
            map.insert(String::from(*cls_name), fn_map);
        }
        map
    }
}

struct DevPM {
    table: ClsRoutingTable,
    rx: tokio::sync::watch::Receiver<Result<ClsRouting, Status>>,
    _tx: tokio::sync::watch::Sender<Result<ClsRouting, Status>>,
}

impl DevPM {
    fn new(cls_map: DashMap<String, DashMap<String, String>>) -> Self {
        let table = create_table(cls_map);
        let c = ClsRouting {
            name: "".into(),
            partitions: 0,
            routing: vec![],
        };
        let (tx, rx) = watch::channel(Ok(c));
        Self { rx, _tx: tx, table }
    }
}

fn create_table(
    cls_map: DashMap<String, DashMap<String, String>>,
) -> ClsRoutingTable {
    let mut cls_routings: Vec<ClsRouting> = Vec::new();
    for entry in cls_map.iter() {
        let mut func_routing: HashMap<String, FuncRouting> = HashMap::new();
        for fn_entry in entry.iter() {
            func_routing.insert(
                fn_entry.key().clone(),
                FuncRouting {
                    url: fn_entry.clone(),
                },
            );
        }
        let partition: PartitionRouting = PartitionRouting {
            functions: func_routing,
            ..Default::default()
        };
        let cls = ClsRouting {
            name: entry.key().clone(),
            partitions: 1,
            routing: vec![partition],
        };
        cls_routings.push(cls);
    }
    let table = ClsRoutingTable { clss: cls_routings };
    table
}

#[tonic::async_trait]
impl RoutingService for DevPM {
    async fn get_cls_routing(
        &self,
        _request: Request<ClsRoutingRequest>,
    ) -> Result<Response<ClsRoutingTable>, tonic::Status> {
        Ok(Response::new(self.table.clone()))
    }

    type WatchClsRoutingStream = WatchStream<Result<ClsRouting, Status>>;

    async fn watch_cls_routing(
        &self,
        _request: Request<ClsRoutingRequest>,
    ) -> Result<Response<Self::WatchClsRoutingStream>, tonic::Status> {
        let rx = self.rx.clone();
        let rx_stream = WatchStream::from_changes(rx);
        Ok(Response::new(rx_stream))
    }
}

#[test]
fn test() {
    let conf = DevConfig {
        cls_list: "example!echo=localhost:8080|other=localhost:8080,".into(),
    };
    let map = conf.get_cls_list();
    print!("{:?}", map);
    assert!(map.contains_key("example"));
}
