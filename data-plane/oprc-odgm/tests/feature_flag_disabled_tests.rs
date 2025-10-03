use oprc_grpc::{data_service_client::DataServiceClient, SetObjectRequest, ObjData, ValData, ValType, CreateCollectionRequest, SingleObjectRequest};
use oprc_odgm::{OdgmConfig, start_server};
use tonic::Code;

fn build_collection_req(name: &str) -> CreateCollectionRequest {
    CreateCollectionRequest {
        name: name.to_string(),
        partition_count: 1,
        replica_count: 1,
        shard_type: "basic".into(),
        shard_assignments: vec![],
        options: std::collections::HashMap::new(),
        invocations: None,
    }
}

async fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn string_ids_disabled_returns_unimplemented() {
    let port = free_port().await;
    let mut cfg = OdgmConfig::default();
    cfg.http_port = port;
    cfg.node_id = Some(11);
    cfg.members = Some("11".into());
    cfg.enable_string_ids = false; // disable feature under test
    cfg.enable_string_entry_keys = true;

    let (odgm, _pool) = start_server(&cfg).await.expect("start odgm");
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let coll = format!("ff_strid_off_{}", nanoid::nanoid!(6));
    odgm.metadata_manager.create_collection(build_collection_req(&coll)).await.expect("create collection");

    // Capabilities should reflect disabled string_ids
    let mut client = DataServiceClient::connect(format!("http://127.0.0.1:{}", port)).await.unwrap();
    let caps = client.capabilities(oprc_grpc::CapabilitiesRequest{}).await.unwrap().into_inner();
    assert!(!caps.string_ids, "expected string_ids capability false");

    // Attempt to create object with string id
    let obj = ObjData { ..Default::default() };
    let req = SetObjectRequest { cls_id: coll.clone(), partition_id: 0, object_id: 0, object: Some(obj), object_id_str: Some("user-x".into()) };
    let err = client.set(req).await.expect_err("expected UNIMPLEMENTED for string id when disabled");
    assert_eq!(err.code(), Code::Unimplemented, "wrong status code: {:?}", err);

    // Numeric still works
    let obj2 = ObjData { ..Default::default() };
    let req2 = SetObjectRequest { cls_id: coll.clone(), partition_id: 0, object_id: 42, object: Some(obj2), object_id_str: None };
    client.set(req2).await.expect("numeric set should succeed");
    let get = client.get(SingleObjectRequest { cls_id: coll, partition_id: 0, object_id: 42, object_id_str: None }).await.expect("get numeric");
    assert!(get.into_inner().obj.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn string_entry_keys_disabled_returns_unimplemented() {
    let port = free_port().await;
    let mut cfg = OdgmConfig::default();
    cfg.http_port = port;
    cfg.node_id = Some(12);
    cfg.members = Some("12".into());
    cfg.enable_string_ids = true;
    cfg.enable_string_entry_keys = false; // disable entry keys

    let (odgm, _pool) = start_server(& cfg).await.expect("start odgm");
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let coll = format!("ff_strentry_off_{}", nanoid::nanoid!(6));
    odgm.metadata_manager.create_collection(build_collection_req(&coll)).await.expect("create collection");

    let mut client = DataServiceClient::connect(format!("http://127.0.0.1:{}", port)).await.unwrap();
    let caps = client.capabilities(oprc_grpc::CapabilitiesRequest{}).await.unwrap().into_inner();
    assert!(!caps.string_entry_keys, "expected string_entry_keys capability false");

    // Attempt to create object with string entry keys (numeric id)
    let mut entries_str = std::collections::HashMap::new();
    entries_str.insert("name".to_string(), ValData { data: b"alice".to_vec().into(), r#type: ValType::Byte as i32 });
    let obj = ObjData { entries: Default::default(), entries_str, metadata: None, event: None };
    let req = SetObjectRequest { cls_id: coll.clone(), partition_id: 0, object_id: 100, object: Some(obj), object_id_str: None };
    let err = client.set(req).await.expect_err("expected UNIMPLEMENTED for string entry keys when disabled");
    assert_eq!(err.code(), Code::Unimplemented);

    // Creating object without string entry keys should succeed
    let obj2 = ObjData { ..Default::default() };
    client.set(SetObjectRequest { cls_id: coll.clone(), partition_id: 0, object_id: 101, object: Some(obj2), object_id_str: None }).await.expect("set without entries_str");
    let get = client.get(SingleObjectRequest { cls_id: coll, partition_id: 0, object_id: 101, object_id_str: None }).await.expect("get ok");
    assert!(get.into_inner().obj.is_some());
}
