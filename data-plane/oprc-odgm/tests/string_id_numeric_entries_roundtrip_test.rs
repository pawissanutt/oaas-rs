use crate::common::{TestConfig, TestEnvironment};
use oprc_grpc::{
    ObjData, SetObjectRequest, SingleObjectRequest, ValData, ValType,
    data_service_client::DataServiceClient,
};
use std::collections::HashMap;
use tonic::transport::Channel;

mod common;

async fn make_client(env: &TestEnvironment) -> DataServiceClient<Channel> {
    let addr = format!("http://127.0.0.1:{}", env.config.odgm_config.http_port);
    DataServiceClient::connect(addr).await.expect("connect")
}

fn val(s: &str) -> ValData {
    ValData {
        data: s.as_bytes().to_vec(),
        r#type: ValType::Byte as i32,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn string_id_numeric_entries_roundtrip() {
    // Reproduces reported scenario: setting numeric entries under an alphabetic string object ID should roundtrip.
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("coll")
        .await
        .expect("create collection");
    let mut client = make_client(&env).await;

    // Build ObjData with numeric entry keys (1,2) and also a string entry to ensure both maps survive.
    let mut entries: HashMap<String, ValData> = HashMap::new();
    entries.insert("1".to_string(), val("alpha"));
    entries.insert("2".to_string(), val("beta"));
    entries.insert("status".into(), val("ok"));

    let obj = ObjData {
        entries,
        ..Default::default()
    };
    let set_req = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("order-aa".into()),
        object: Some(obj),
    };
    client
        .set(set_req)
        .await
        .expect("set string object with entries");

    // Get again
    let get_req = SingleObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("order-aa".into()),
    };
    let resp = client.get(get_req).await.expect("get object").into_inner();
    let obj_back = resp.obj.expect("object missing");

    // Assert numeric entries present (keys 1,2) and string entry present
    assert_eq!(obj_back.entries.get("1").unwrap().data, b"alpha".to_vec());
    assert_eq!(obj_back.entries.get("2").unwrap().data, b"beta".to_vec());
    assert_eq!(
        obj_back.entries.get("status").unwrap().data,
        b"ok".to_vec()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn numeric_id_vs_string_id_consistency() {
    // Compare legacy numeric path vs string path when ID is numeric-looking.
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("coll")
        .await
        .expect("create collection");
    let mut client = make_client(&env).await;

    // Prepare object
    let mut entries: HashMap<String, ValData> = HashMap::new();
    entries.insert("7".to_string(), val("value-seven"));
    let obj = ObjData {
        entries: entries.clone(),
        ..Default::default()
    };

    // Legacy numeric path (object_id provided)
    let set_numeric = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("99".into()),
        object: Some(obj.clone()),
    };
    client
        .set(set_numeric)
        .await
        .expect("set numeric id object");
    let get_numeric = SingleObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("99".into()),
    };
    let resp_num = client
        .get(get_numeric)
        .await
        .expect("get numeric id object")
        .into_inner();
    let obj_num = resp_num.obj.expect("missing numeric object");
    assert_eq!(
        obj_num.entries.get("7").unwrap().data,
        b"value-seven".to_vec()
    );

    // String path with same numeric-looking ID but passed as string (object_id=0, object_id_str Some)
    let set_string = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("99".into()),
        object: Some(obj.clone()),
    };
    client.set(set_string).await.expect("set string id object");
    let get_string = SingleObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("99".into()),
    };
    let resp_str = client
        .get(get_string)
        .await
        .expect("get string id object")
        .into_inner();
    let obj_str = resp_str.obj.expect("missing string object");
    assert_eq!(
        obj_str.entries.get("7").unwrap().data,
        b"value-seven".to_vec()
    );
}
