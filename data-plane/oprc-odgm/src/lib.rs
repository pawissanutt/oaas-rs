mod cluster;
pub mod error;
pub mod events;
pub mod granular_key;
pub mod granular_trait;
mod grpc_service;
pub mod identity; // string ID normalization & ObjectIdentity
pub mod metadata;
pub mod metrics;
pub mod replication;
pub mod shard;
pub mod storage_key;

use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

pub use cluster::ObjectDataGridManager;
use envconfig::Envconfig;
use grpc_service::OdgmDataService;
use metadata::OprcMetaManager;
use oprc_grpc::{
    data_service_server::DataServiceServer,
    oprc_function_server::OprcFunctionServer,
};
// Bring CreateCollectionRequest only when building with serde or always? Always needed for env loading.
use oprc_grpc::CreateCollectionRequest;
pub mod collection_helpers;
use oprc_zenoh::pool::Pool;
use shard::{UnifiedShardFactory, UnifiedShardManager};
use tracing::info;

#[derive(Envconfig, Clone, Debug)]
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
    #[envconfig(from = "ODGM_MAX_SESSIONS", default = "1")]
    pub max_sessions: u16,
    #[envconfig(from = "ODGM_REFLECTION_ENABLED", default = "false")]
    pub reflection_enabled: bool,
    #[envconfig(from = "ODGM_EVENTS_ENABLED", default = "true")]
    pub events_enabled: bool,
    #[envconfig(from = "ODGM_MAX_TRIGGER_DEPTH", default = "10")]
    pub max_trigger_depth: u32,
    #[envconfig(from = "ODGM_TRIGGER_TIMEOUT_MS", default = "30000")]
    pub trigger_timeout_ms: u64,
    #[envconfig(from = "ODGM_MAX_STRING_ID_LEN", default = "160")]
    pub max_string_id_len: usize,
    #[envconfig(from = "ODGM_ENABLE_STRING_IDS", default = "true")]
    pub enable_string_ids: bool,
    #[envconfig(from = "ODGM_ENABLE_STRING_ENTRY_KEYS", default = "true")]
    pub enable_string_entry_keys: bool,
    #[envconfig(from = "ODGM_ENABLE_GRANULAR_STORAGE", default = "false")]
    pub enable_granular_entry_storage: bool,
    #[envconfig(from = "ODGM_GRANULAR_PREFETCH_LIMIT", default = "256")]
    pub granular_prefetch_limit: usize,
}

impl Default for OdgmConfig {
    fn default() -> Self {
        Self {
            http_port: 8080,
            max_sessions: 1,
            node_id: None,
            node_addr: None,
            collection: None,
            members: None,
            reflection_enabled: false,
            events_enabled: true,
            max_trigger_depth: 10,
            trigger_timeout_ms: 30000,
            max_string_id_len: 160,
            enable_string_ids: true,
            enable_string_entry_keys: true,
            enable_granular_entry_storage: false,
            granular_prefetch_limit: 256,
        }
    }
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
) -> Result<(ObjectDataGridManager, Pool), Box<dyn Error>> {
    let z_conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    // let z_session = zenoh::open(zenoh_conf.create_zenoh()).await.unwrap();

    let session_pool = Pool::new(conf.max_sessions as usize, z_conf);
    let node_id = conf.get_node_id();
    let metadata_manager = OprcMetaManager::new(node_id, conf.get_members());
    let metadata_manager = Arc::new(metadata_manager);

    let shard_factory = if conf.events_enabled {
        let event_config = crate::events::EventConfig {
            max_trigger_depth: conf.max_trigger_depth,
            trigger_timeout_ms: conf.trigger_timeout_ms,
            ..Default::default()
        };
        Arc::new(UnifiedShardFactory::new_with_events(
            session_pool.clone(),
            event_config,
            conf.enable_string_ids,
            conf.max_string_id_len,
        ))
    } else {
        Arc::new(UnifiedShardFactory::new(
            session_pool.clone(),
            conf.enable_string_ids,
            conf.max_string_id_len,
        ))
    };

    let shard_manager = Arc::new(UnifiedShardManager::new(shard_factory));
    let odgm = ObjectDataGridManager::new(
        node_id,
        metadata_manager.clone(),
        shard_manager,
    )
    .await;
    odgm.start_watch_stream();
    return Ok((odgm, session_pool));
}

pub async fn start_server(
    conf: &OdgmConfig,
) -> Result<(Arc<ObjectDataGridManager>, Pool), Box<dyn Error>> {
    let (odgm, session_pool) = start_raw_server(conf).await?;
    let odgm = Arc::new(odgm);

    let data_service = OdgmDataService::new(
        odgm.clone(),
        conf.max_string_id_len,
        conf.enable_string_ids,
        conf.enable_string_entry_keys,
        conf.enable_granular_entry_storage,
        conf.granular_prefetch_limit,
    );
    let z_session = session_pool.get_session().await.unwrap();
    let invocation_service =
        grpc_service::InvocationService::new(odgm.clone(), z_session);
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);

    let odgm_: Arc<ObjectDataGridManager> = odgm.clone();
    let refection_enabled = conf.reflection_enabled;
    tokio::spawn(async move {
        let mut builder = tonic::transport::Server::builder();
        if refection_enabled {
            let reflection_server_v1a =
                tonic_reflection::server::Builder::configure()
                    .register_encoded_file_descriptor_set(
                        oprc_grpc::FILE_DESCRIPTOR_SET,
                    )
                    .build_v1alpha()
                    .unwrap();

            let reflection_server_v1 =
                tonic_reflection::server::Builder::configure()
                    .register_encoded_file_descriptor_set(
                        oprc_grpc::FILE_DESCRIPTOR_SET,
                    )
                    .build_v1()
                    .unwrap();
            builder
                .add_service(reflection_server_v1a)
                .add_service(reflection_server_v1);
        }
        builder
            .add_service(DataServiceServer::new(data_service))
            .add_service(OprcFunctionServer::new(invocation_service))
            .serve_with_shutdown(socket, shutdown_signal(odgm_))
            .await
            .unwrap();
    });
    info!("start on {}", socket);

    Ok((odgm, session_pool))
}

async fn shutdown_signal(odgm: Arc<ObjectDataGridManager>) {
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
    odgm.close().await;
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

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use crate::{
        ObjectDataGridManager, OdgmConfig,
        metadata::OprcMetaManager,
        shard::{UnifiedShardFactory, UnifiedShardManager},
    };

    #[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 4))]
    async fn test_close() {
        let conf = OdgmConfig {
            node_id: Some(1),
            members: Some("1".into()),
            ..Default::default()
        };

        let z_conf = oprc_zenoh::OprcZenohConfig::default();
        let session_pool =
            oprc_zenoh::pool::Pool::new(conf.max_sessions as usize, z_conf);
        let node_id = conf.get_node_id();
        let metadata_manager =
            OprcMetaManager::new(node_id, conf.get_members());
        let metadata_manager = Arc::new(metadata_manager);

        let shard_factory = if conf.events_enabled {
            let event_config = crate::events::EventConfig {
                max_trigger_depth: conf.max_trigger_depth,
                trigger_timeout_ms: conf.trigger_timeout_ms,
                ..Default::default()
            };
            Arc::new(UnifiedShardFactory::new_with_events(
                session_pool,
                event_config,
                conf.enable_string_ids,
                conf.max_string_id_len,
            ))
        } else {
            Arc::new(UnifiedShardFactory::new(
                session_pool,
                conf.enable_string_ids,
                conf.max_string_id_len,
            ))
        };
        let shard_manager = Arc::new(UnifiedShardManager::new(shard_factory));
        let odgm = ObjectDataGridManager::new(
            node_id,
            metadata_manager.clone(),
            shard_manager.clone(),
        )
        .await;
        odgm.start_watch_stream();

        metadata_manager
            .create_collection(
                crate::collection_helpers::minimal_mst_with_echo("test"),
            )
            .await
            .unwrap();

        // Wait for shard creation to complete by polling the stats
        let mut attempts = 0;
        while attempts < 100 {
            let stats = shard_manager.get_stats().await;
            if stats.total_shards_created >= 1 {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        }

        assert_eq!(shard_manager.get_stats().await.total_shards_created, 1);
        odgm.close().await;
    }
}
