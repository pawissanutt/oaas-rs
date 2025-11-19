use std::sync::Arc;
use tokio::time::{Duration, sleep};

use oprc_grpc::{CreateCollectionRequest, ObjData, ObjMeta, ValData};
use oprc_invoke::proxy::ObjectProxy;
use oprc_zenoh::{OprcZenohConfig, pool::Pool};

use oprc_odgm::ObjectDataGridManager;
use oprc_odgm::metadata::OprcMetaManager;
use oprc_odgm::shard::{
    UnifiedShardConfig, UnifiedShardFactory, UnifiedShardManager,
};

fn pick_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn zenoh_string_id_roundtrip() {
    // Build zenoh pool explicitly
    let port = pick_free_port();
    let mut z_conf = OprcZenohConfig::default();
    z_conf.zenoh_port = port;
    z_conf.gossip_enabled = Some(true);
    let pool = Pool::new(8, z_conf);

    // Build ODGM managers
    let node_id = 1u64;
    let members = vec![node_id];
    let metadata = Arc::new(OprcMetaManager::new(node_id, members));
    let factory_cfg = UnifiedShardConfig {
        max_string_id_len: 64,
        granular_prefetch_limit: 256,
    };
    let factory = Arc::new(UnifiedShardFactory::new(pool.clone(), factory_cfg));
    let shard_manager = Arc::new(UnifiedShardManager::new(factory));
    let odgm = ObjectDataGridManager::new(
        node_id,
        metadata.clone(),
        shard_manager.clone(),
    )
    .await;
    odgm.start_watch_stream();

    // Create a collection
    let collection = format!("odgm_zenoh_str_{}", nanoid::nanoid!(6));
    let req = CreateCollectionRequest {
        name: collection.clone(),
        partition_count: 1,
        replica_count: 1,
        shard_type: "basic".into(),
        shard_assignments: vec![],
        options: Default::default(),
        invocations: None,
    };
    odgm.metadata_manager
        .create_collection(req)
        .await
        .expect("create collection");

    // Wait for shard readiness
    let mut attempts = 0;
    while attempts < 100 {
        let shards = shard_manager.get_shards_for_collection(&collection).await;
        if !shards.is_empty()
            && shards.iter().all(|s| *s.watch_readiness().borrow())
        {
            break;
        }
        sleep(Duration::from_millis(50)).await;
        attempts += 1;
    }

    // Use ObjectProxy over zenoh
    let session = pool.get_session().await.expect("zenoh session");
    let proxy = ObjectProxy::new(session);

    // Set object with string ID
    let mut obj = ObjData::default();
    obj.entries.insert(
        "name".into(),
        ValData {
            data: b"alice".to_vec(),
            ..Default::default()
        },
    );
    obj.metadata = Some(ObjMeta {
        cls_id: collection.clone(),
        partition_id: 0,
        object_id: Some("user-alpha".into()),
    });
    proxy.set_obj(obj).await.expect("set_obj");

    // Get object back
    let meta = ObjMeta {
        cls_id: collection.clone(),
        partition_id: 0,
        object_id: Some("user-alpha".into()),
    };
    let got = proxy.get_obj(&meta).await.expect("get_obj");
    let Some(obj) = got else {
        panic!("expected object");
    };
    assert_eq!(
        String::from_utf8_lossy(&obj.entries.get("name").unwrap().data),
        "alice"
    );
}
