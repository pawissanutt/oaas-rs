mod metadata;
mod network;
mod replication;
mod shard;

use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use envconfig::Envconfig;
use flare_dht::{
    cli::ServerArgs,
    pool::{create_control_pool, create_data_pool, AddrResolver},
    shard::ShardManager,
    FlareNode,
};
use flare_pb::CreateCollectionRequest;
use metadata::OprcMetaManager;
use network::OdgmDataService;
use oprc_pb::data_service_server::DataServiceServer;
use shard::{ObjectShard, ObjectShardFactory};
use tracing::info;

#[derive(Envconfig, Clone, Debug)]
pub struct Config {
    #[envconfig(from = "ODGM_HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "ODGM_CLASS")]
    pub class: Option<String>,
    #[envconfig(from = "ODGM_PARTITION_ID")]
    pub partition: Option<u16>,
    #[envconfig(from = "ODGM_REPLICA_ID")]
    pub replica: Option<u16>,
}

struct OprcAddrResolver {
    metadata_manager: Arc<OprcMetaManager>,
}

#[async_trait::async_trait]
impl AddrResolver for OprcAddrResolver {
    async fn resolve(&self, node_id: u64) -> Option<String> {
        return self
            .metadata_manager
            .membership
            .get(&node_id)
            .map(|node| node.node_addr.clone());
    }
}

pub async fn start_server(
    conf: &Config,
) -> Result<Arc<FlareNode<ObjectShard>>, Box<dyn Error>> {
    let server_args = ServerArgs {
        ..Default::default()
    };
    let node_id = server_args.get_node_id();
    let metadata_manager =
        OprcMetaManager::new(node_id, server_args.get_addr());
    let metadata_manager = Arc::new(metadata_manager);
    let shard_manager =
        Arc::new(ShardManager::new(Box::new(ObjectShardFactory {})));
    let resolver = OprcAddrResolver {
        metadata_manager: metadata_manager.clone(),
    };
    let resolver = Arc::new(resolver);
    let flare_node = FlareNode::new(
        server_args.get_addr(),
        node_id,
        metadata_manager.clone(),
        shard_manager,
        Arc::new(create_control_pool(resolver.clone())),
        Arc::new(create_data_pool(resolver.clone())),
    )
    .await;
    let flare_node = Arc::new(flare_node);
    flare_node.clone().start_watch_stream();

    let data_service = OdgmDataService::new(flare_node.clone());
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
            .serve(socket)
            .await
            .unwrap();
    });
    info!("start on {}", socket);

    Ok(flare_node)
}

pub async fn create_collection(
    flare: Arc<FlareNode<ObjectShard>>,
    conf: &Config,
) {
    let node_id = flare.node_id;
    if let Some(cls) = &conf.class {
        let cls_list = cls.split(",");
        for cls_key in cls_list {
            tracing::info!("create class collection: {}", cls_key);
            let req = CreateCollectionRequest {
                name: cls_key.into(),
                shard_count: 1,
                shard_assignments: vec![node_id],
            };
            flare.metadata_manager.create_collection(req).await.unwrap();
        }
    }
}
