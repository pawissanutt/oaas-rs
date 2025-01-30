mod cluster;
pub mod error;
mod grpc_service;
pub mod metadata;
pub mod shard;

use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

pub use cluster::ObjectDataGridManager;
use envconfig::Envconfig;
use grpc_service::OdgmDataService;
use metadata::OprcMetaManager;
use oprc_pb::{
    data_service_server::DataServiceServer, CreateCollectionRequest,
};
use shard::{factory::UnifyShardFactory, manager::ShardManager};
use tracing::info;

#[derive(Envconfig, Clone, Debug, Default)]
pub struct OdgmConfig {
    #[envconfig(from = "ODGM_HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "ODGM_NODE_ID")]
    pub node_id: Option<u64>,
    #[envconfig(from = "ODGM_NODE_ADDR")]
    pub node_addr: Option<String>,
    #[envconfig(from = "ODGM_COLLECTION")]
    pub collection: Option<String>,
    #[envconfig(from = "ODGM_MEMBERS")]
    pub members: Option<String>,
    #[envconfig(from = "ODGM_MAX_SESSIONS", default = "3")]
    pub max_sessions: u16,
}

impl OdgmConfig {
    pub fn get_node_id(&self) -> u64 {
        if let Some(id) = self.node_id {
            return id;
        }
        rand::random()
    }

    pub fn get_addr(&self) -> String {
        if let Some(addr) = &self.node_addr {
            return addr.clone();
        }
        return format!("http://127.0.0.1:{}", self.http_port);
    }
}

impl OdgmConfig {
    fn get_members(&self) -> Vec<u64> {
        if let Some(members_str) = &self.members {
            let members: Vec<u64> = members_str
                .split(",")
                .map(|s| s.parse::<u64>().unwrap())
                .collect();
            return members;
        } else {
            return vec![self.node_id.unwrap_or_else(|| rand::random::<u64>())];
        }
    }
}

pub async fn start_raw_server(
    conf: &OdgmConfig,
) -> Result<Arc<ObjectDataGridManager>, Box<dyn Error>> {
    let zenoh_conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    // let z_session = zenoh::open(zenoh_conf.create_zenoh()).await.unwrap();

    let node_id = conf.get_node_id();
    let metadata_manager = OprcMetaManager::new(node_id, conf.get_members());
    let metadata_manager = Arc::new(metadata_manager);
    let shard_factory =
        UnifyShardFactory::new(zenoh_conf.clone(), conf.clone());
    let shard_manager = Arc::new(ShardManager::new(Box::new(shard_factory)));
    let odgm = ObjectDataGridManager::new(
        conf.get_addr(),
        node_id,
        metadata_manager.clone(),
        shard_manager,
    )
    .await;
    let odgm = Arc::new(odgm);
    odgm.clone().start_watch_stream();
    return Ok(odgm);
}

pub async fn start_server(
    conf: &OdgmConfig,
) -> Result<Arc<ObjectDataGridManager>, Box<dyn Error>> {
    let odgm = start_raw_server(conf).await?;

    let data_service = OdgmDataService::new(odgm.clone());
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);

    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(reflection_server_v1a)
            .add_service(reflection_server_v1)
            .add_service(DataServiceServer::new(data_service))
            .serve_with_shutdown(socket, shutdown_signal())
            .await
            .unwrap();
    });
    info!("start on {}", socket);

    Ok(odgm)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
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
}

pub async fn create_collection(
    ogdm: Arc<ObjectDataGridManager>,
    conf: &OdgmConfig,
) {
    if let Some(collection_str) = &conf.collection {
        let collection_reqs: Vec<CreateCollectionRequest> =
            serde_json::from_str(&collection_str).unwrap();
        info!(
            "load collection from env. found {} collections",
            collection_reqs.len()
        );
        for req in collection_reqs.iter() {
            info!("Apply collection '{}' from env", req.name);
            ogdm.metadata_manager
                .create_collection(req.clone())
                .await
                .unwrap();
        }
    }
}
