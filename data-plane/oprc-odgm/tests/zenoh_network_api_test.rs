mod common;

use std::time::Duration;
use std::{collections::HashMap, convert::TryInto};

use common::{TestConfig, TestEnvironment};
use flume::bounded;
use oprc_grpc::{
    BatchSetValuesRequest, EmptyResponse, ObjData, ObjMeta,
    SingleObjectRequest, ValData, ValType,
    data_service_client::DataServiceClient,
};
use oprc_invoke::proxy::ObjectProxy;
use prost::Message;
use tokio::time::sleep;
use tonic::transport::Channel;
use zenoh::{
    bytes::ZBytes, key_expr::KeyExpr, qos::CongestionControl,
    query::ConsolidationMode,
};

const PARTITION_ID: u16 = 0;

async fn make_client(env: &TestEnvironment) -> DataServiceClient<Channel> {
    let addr = format!("http://127.0.0.1:{}", env.config.odgm_config.http_port);
    DataServiceClient::connect(addr)
        .await
        .expect("failed to connect gRPC client")
}

async fn issue_set_query(
    session: &zenoh::Session,
    key_expr: &str,
    payload: Vec<u8>,
) -> zenoh::sample::Sample {
    let key_expr: KeyExpr = key_expr
        .try_into()
        .expect("invalid key expression for set query");
    let (tx, rx) = bounded(1);
    session
        .get(&key_expr)
        .payload(ZBytes::from(payload))
        .consolidation(ConsolidationMode::None)
        .congestion_control(CongestionControl::Block)
        .callback(move |reply| {
            let _ = tx.send(reply);
        })
        .await
        .expect("failed to issue set query");
    let reply = rx.recv_async().await.expect("failed to receive set reply");
    reply.result().expect("set query returned error").clone()
}

async fn fetch_object_via_zenoh(
    session: &zenoh::Session,
    key_expr: &str,
) -> Result<ObjData, String> {
    let key_expr: KeyExpr = key_expr
        .try_into()
        .expect("invalid key expression for get query");
    let replies = session
        .get(&key_expr)
        .await
        .expect("failed to issue get query");
    let reply = replies
        .recv_async()
        .await
        .map_err(|e| format!("failed to receive reply: {}", e))?;
    match reply.result() {
        Ok(sample) => ObjData::decode(sample.payload().to_bytes().as_ref())
            .map_err(|e| format!("failed to decode object data: {}", e)),
        Err(err) => Err(format!("query returned error: {:?}", err)),
    }
}

fn build_obj_data_with_numeric_id(
    cls_id: &str,
    object_id: u64,
    value: &[u8],
) -> ObjData {
    let mut entries = HashMap::new();
    entries.insert(
        1,
        ValData {
            data: value.to_vec(),
            r#type: ValType::Byte as i32,
        },
    );

    ObjData {
        metadata: Some(ObjMeta {
            cls_id: cls_id.to_string(),
            partition_id: PARTITION_ID as u32,
            object_id,
            object_id_str: None,
        }),
        entries,
        ..Default::default()
    }
}

fn build_obj_data_with_string_id(
    cls_id: &str,
    object_id_str: &str,
    value: &[u8],
) -> ObjData {
    let mut entries_str = HashMap::new();
    entries_str.insert(
        "status".to_string(),
        ValData {
            data: value.to_vec(),
            r#type: ValType::Byte as i32,
        },
    );

    ObjData {
        metadata: Some(ObjMeta {
            cls_id: cls_id.to_string(),
            partition_id: PARTITION_ID as u32,
            object_id: 0,
            object_id_str: Some(object_id_str.to_string()),
        }),
        entries_str,
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn zenoh_set_get_string_id_roundtrip() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("zenoh_coll")
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let mut client = make_client(&env).await;

    let object_id_str = "order-alpha-42";
    let payload = b"string-object-payload";
    let obj =
        build_obj_data_with_string_id("zenoh_coll", object_id_str, payload);

    let set_path = format!(
        "oprc/{}/{}/objects/{}/set",
        "zenoh_coll", PARTITION_ID, object_id_str
    );
    let sample =
        issue_set_query(&session, &set_path, obj.clone().encode_to_vec()).await;
    EmptyResponse::decode(sample.payload().to_bytes().as_ref())
        .expect("set path should return EmptyResponse");

    let get_path = format!(
        "oprc/{}/{}/objects/{}",
        "zenoh_coll", PARTITION_ID, object_id_str
    );

    // String IDs via Zenoh GET should return an error directing to gRPC
    let result = fetch_object_via_zenoh(&session, &get_path).await;
    assert!(
        result.is_err(),
        "zenoh get for string IDs should return error, got: {:?}",
        result
    );
    let err_msg = format!("{}", result.unwrap_err());
    // The error message is in the ZBytes payload, check for key parts
    assert!(
        err_msg.contains("String ID")
            || err_msg.contains("53, 74, 72, 69, 6e, 67"),
        "error should indicate string ID limitation, got: {}",
        err_msg
    );

    // Verify the object can be retrieved via gRPC instead
    let response = client
        .get(SingleObjectRequest {
            cls_id: "zenoh_coll".into(),
            partition_id: PARTITION_ID as u32,
            object_id: 0,
            object_id_str: Some(object_id_str.into()),
        })
        .await
        .expect("gRPC get string id")
        .into_inner();
    let grpc_obj = response.obj.expect("gRPC object missing");
    let grpc_value = grpc_obj
        .entries_str
        .get("status")
        .expect("gRPC string entry missing");
    assert_eq!(grpc_value.data, payload);

    env.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn zenoh_put_numeric_id_roundtrip() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("zenoh_coll")
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let mut client = make_client(&env).await;

    let object_id = 777_u64;
    let payload = b"numeric-object-payload";
    let obj = build_obj_data_with_numeric_id("zenoh_coll", object_id, payload);

    let put_path = format!(
        "oprc/{}/{}/objects/{}",
        "zenoh_coll", PARTITION_ID, object_id
    );
    let key_expr: KeyExpr = put_path
        .clone()
        .try_into()
        .expect("invalid key expression for put");
    session
        .put(&key_expr, obj.clone().encode_to_vec())
        .await
        .expect("zenoh put failed");

    sleep(Duration::from_millis(200)).await;

    let get_path = format!(
        "oprc/{}/{}/objects/{}",
        "zenoh_coll", PARTITION_ID, object_id
    );
    let fetched = fetch_object_via_zenoh(&session, &get_path)
        .await
        .unwrap_or_else(|err| {
            panic!("zenoh get should return object: {}", err)
        });
    let value = fetched.entries.get(&1).expect("numeric entry missing");
    assert_eq!(value.data, payload);

    let response = client
        .get(SingleObjectRequest {
            cls_id: "zenoh_coll".into(),
            partition_id: PARTITION_ID as u32,
            object_id,
            object_id_str: None,
        })
        .await
        .expect("gRPC get numeric id")
        .into_inner();
    let grpc_obj = response.obj.expect("gRPC object missing");
    let grpc_value = grpc_obj.entries.get(&1).expect("gRPC entry missing");
    assert_eq!(grpc_value.data, payload);

    env.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn zenoh_batch_set_and_entry_get() {
    let mut cfg = TestConfig::new().await;
    cfg.odgm_config.enable_granular_entry_storage = true;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("zenoh_coll")
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let proxy = ObjectProxy::new(session.clone());

    let object_id_str = "order-batch-001";
    let mut values = HashMap::new();
    values.insert(
        "status".to_string(),
        ValData {
            data: b"pending".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );
    values.insert(
        "notes".to_string(),
        ValData {
            data: b"urgent".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );

    proxy
        .batch_set_entries(&BatchSetValuesRequest {
            cls_id: "zenoh_coll".into(),
            partition_id: PARTITION_ID as u32,
            object_id: 0,
            object_id_str: Some(object_id_str.into()),
            values,
            delete_keys: vec![],
            expected_object_version: None,
        })
        .await
        .expect("batch set should succeed");

    let meta = ObjMeta {
        cls_id: "zenoh_coll".into(),
        partition_id: PARTITION_ID as u32,
        object_id: 0,
        object_id_str: Some(object_id_str.into()),
    };

    let status_entry = proxy
        .get_entry(&meta, "status")
        .await
        .expect("get entry via zenoh")
        .expect("status entry should exist after batch");
    let status_val = status_entry
        .value
        .expect("status entry should include value");
    assert_eq!(status_val.data, b"pending");
    assert_eq!(status_entry.object_version, Some(1));

    let mut update_values = HashMap::new();
    update_values.insert(
        "status".to_string(),
        ValData {
            data: b"complete".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );

    proxy
        .batch_set_entries(&BatchSetValuesRequest {
            cls_id: "zenoh_coll".into(),
            partition_id: PARTITION_ID as u32,
            object_id: 0,
            object_id_str: Some(object_id_str.into()),
            values: update_values,
            delete_keys: vec!["notes".into()],
            expected_object_version: Some(1),
        })
        .await
        .expect("second batch set should succeed");

    let updated_status = proxy
        .get_entry(&meta, "status")
        .await
        .expect("get entry after update")
        .expect("status entry should still exist");
    let updated_val = updated_status
        .value
        .expect("status value should be present");
    assert_eq!(updated_val.data, b"complete");
    assert_eq!(updated_status.object_version, Some(3));

    let notes_entry = proxy
        .get_entry(&meta, "notes")
        .await
        .expect("get entry for deleted key");
    assert!(notes_entry.is_none(), "notes should be deleted");

    proxy.close().await.expect("close proxy");
    env.shutdown().await;
}
