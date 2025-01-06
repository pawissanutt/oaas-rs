mod cluster;
mod metadata;
mod network;
mod replication;
// mod resolver;
mod shard;
mod zrpc;

use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use cluster::ObjectDataGridManager;
use envconfig::Envconfig;
use flare_dht::{cli::ServerArgs, shard::ShardManager2};
use metadata::OprcMetaManager;
use network::OdgmDataService;
use oprc_pb::data_service_server::DataServiceServer;
use shard::UnifyShardFactory;
use tracing::info;

#[derive(Envconfig, Clone, Debug)]
pub struct Config {
    #[envconfig(from = "ODGM_HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "ODGM_NODE_ID")]
    pub node_id: Option<u64>,
    #[envconfig(from = "ODGM_COLLECTION")]
    pub collection: Option<String>,
}

pub async fn start_server(
    conf: &Config,
) -> Result<Arc<ObjectDataGridManager>, Box<dyn Error>> {
    let server_args = ServerArgs {
        node_id: conf.node_id,
        ..Default::default()
    };

    let zenoh_conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let z_session = zenoh::open(zenoh_conf.create_zenoh()).await.unwrap();

    let node_id = server_args.get_node_id();
    let metadata_manager =
        OprcMetaManager::new(node_id, server_args.get_addr());
    let metadata_manager = Arc::new(metadata_manager);
    let shard_manager = Arc::new(ShardManager2::new(Box::new(
        UnifyShardFactory::new(z_session),
    )));
    let odgm = ObjectDataGridManager::new(
        server_args.get_addr(),
        node_id,
        metadata_manager.clone(),
        shard_manager,
    )
    .await;
    let flare_node = Arc::new(odgm);
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
    ogdm: Arc<ObjectDataGridManager>,
    conf: &Config,
) {
    if let Some(collection_str) = &conf.collection {
        let collection_req = serde_json::from_str(&collection_str).unwrap();
        ogdm.metadata_manager
            .create_collection(collection_req)
            .await
            .unwrap();
    }
    // let node_id = ogdm.node_id;
    // if let Some(cls) = &conf.class {
    //     let cls_list = cls.split(",");
    //     for cls_key in cls_list {
    //         tracing::info!("create class collection: {}", cls_key);
    //         let req = CreateCollectionRequest {
    //             name: cls_key.into(),
    //             partition_count: 1,
    //             shard_assignments: vec![flare_pb::ShardAssignment {
    //                 primary: node_id,
    //                 ..Default::default()
    //             }],
    //             ..Default::default()
    //         };
    //         ogdm.metadata_manager.create_collection(req).await.unwrap();
    //     }
    // }
}
