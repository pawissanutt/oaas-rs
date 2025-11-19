use std::collections::HashMap;

use oprc_grpc::{
    BatchSetValuesRequest, CapabilitiesRequest, CreateCollectionRequest,
    ListValuesRequest, SetKeyRequest, SingleKeyRequest, ValData, ValType,
    data_service_client::DataServiceClient,
};
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

fn val(data: &str) -> ValData {
    ValData {
        data: data.as_bytes().to_vec(),
        r#type: ValType::Byte as i32,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn granular_rpc_end_to_end() {
    let port = free_port().await;
    let mut cfg = OdgmConfig::default();
    cfg.http_port = port;
    cfg.node_id = Some(99);
    cfg.members = Some("99".into());
    cfg.enable_string_entry_keys = true;
    cfg.enable_granular_entry_storage = true;

    let (odgm, _pool) = start_server(&cfg).await.expect("start odgm");
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let coll =
        format!("granular_rpc_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    odgm.metadata_manager
        .create_collection(build_collection_req(&coll))
        .await
        .expect("create collection");

    // Wait for shard creation to complete to avoid NotFound errors on early RPCs
    let mut tries = 0;
    while tries < 100 {
        let stats = odgm.shard_manager.get_stats().await;
        if stats.total_shards_created >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        tries += 1;
    }

    let mut client =
        DataServiceClient::connect(format!("http://127.0.0.1:{}", port))
            .await
            .expect("connect client");

    let caps = client
        .capabilities(CapabilitiesRequest {})
        .await
        .expect("capabilities")
        .into_inner();
    assert!(caps.granular_entry_storage);

    let object_id = format!("user-{}", nanoid::nanoid!(6).to_ascii_lowercase());

    let mut values = HashMap::new();
    values.insert("profile:name".to_string(), val("Alice"));
    values.insert("profile:email".to_string(), val("alice@example.com"));

    client
        .batch_set_values(BatchSetValuesRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            values,
            delete_keys: vec![],
            expected_object_version: None,
        })
        .await
        .expect("batch set values");

    let name_resp = client
        .get_value(SingleKeyRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key: Some("profile:name".into()),
        })
        .await
        .expect("get value")
        .into_inner();
    let first_version = name_resp.object_version.expect("version present");
    assert_eq!(name_resp.value.unwrap().data, b"Alice".to_vec());

    // Update via batch with CAS
    let mut update_values = HashMap::new();
    update_values.insert("profile:name".to_string(), val("Alice Smith"));
    client
        .batch_set_values(BatchSetValuesRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            values: update_values,
            delete_keys: vec![],
            expected_object_version: Some(first_version),
        })
        .await
        .expect("batch update");

    let name_resp2 = client
        .get_value(SingleKeyRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key: Some("profile:name".into()),
        })
        .await
        .expect("get updated value")
        .into_inner();
    assert_eq!(name_resp2.value.unwrap().data, b"Alice Smith".to_vec());
    let second_version = name_resp2.object_version.expect("version present");
    assert!(second_version > first_version);

    // Set single value through SetValue (granular path)
    client
        .set_value(SetKeyRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key: Some("profile:tier".into()),
            value: Some(val("standard")),
        })
        .await
        .expect("set value");

    let mut list_stream = client
        .list_values(ListValuesRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key_prefix: None,
            limit: 1,
            cursor: None,
        })
        .await
        .expect("list values")
        .into_inner();

    let first_page = list_stream
        .message()
        .await
        .expect("stream result")
        .expect("first envelope");
    assert!(first_page.next_cursor.is_some());
    let first_cursor = first_page.next_cursor.clone();
    drop(list_stream);

    let mut second_stream = client
        .list_values(ListValuesRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key_prefix: None,
            limit: 10,
            cursor: first_cursor,
        })
        .await
        .expect("list page two")
        .into_inner();

    let mut keys = Vec::new();
    while let Some(msg) = second_stream.message().await.expect("stream message")
    {
        if let Some(val) = msg.value {
            keys.push((msg.key.clone(), val.data.clone()));
        }
    }
    assert_eq!(keys.len(), 2);

    client
        .delete_value(SingleKeyRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key: Some("profile:email".into()),
        })
        .await
        .expect("delete value");

    let deleted_resp = client
        .get_value(SingleKeyRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            key: Some("profile:email".into()),
        })
        .await
        .expect("get deleted")
        .into_inner();
    assert!(deleted_resp.value.is_none());

    let err = client
        .batch_set_values(BatchSetValuesRequest {
            cls_id: coll.clone(),
            partition_id: 0,
            object_id: Some(object_id.clone()),
            values: HashMap::new(),
            delete_keys: vec!["profile:ghost".into()],
            expected_object_version: Some(9999),
        })
        .await
        .expect_err("expected aborted on stale version");
    assert_eq!(err.code(), Code::Aborted);

    drop(client);
    odgm.close().await;
}
