use crate::common::{TestConfig, TestEnvironment};
use oprc_grpc::{
    ObjData, SetObjectRequest, SingleObjectRequest,
    data_service_client::DataServiceClient,
};
use tonic::transport::Channel;

mod common;

async fn make_client(env: &TestEnvironment) -> DataServiceClient<Channel> {
    let addr = format!("http://127.0.0.1:{}", env.config.odgm_config.http_port);
    DataServiceClient::connect(addr).await.expect("connect")
}

#[tokio::test(flavor = "multi_thread")]
async fn string_object_create_get_roundtrip() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    // Create collection
    env.create_test_collection("coll")
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Set object via string ID
    let obj = ObjData {
        ..Default::default()
    };
    let req = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: 0,
        object: Some(obj),
        object_id_str: Some("order-abc-1".into()),
    };
    client.set(req).await.expect("set string object");

    // Get object via string ID
    let get_req = SingleObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: 0,
        object_id_str: Some("order-abc-1".into()),
    };
    let resp = client.get(get_req).await.expect("get string object");
    assert!(resp.into_inner().obj.is_some(), "object missing");
}

#[tokio::test(flavor = "multi_thread")]
async fn both_ids_rejected() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("coll")
        .await
        .expect("create collection");
    let mut client = make_client(&env).await;

    let obj = ObjData {
        ..Default::default()
    };
    let req = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: 42,
        object: Some(obj),
        object_id_str: Some("order-x".into()),
    };
    let err = client.set(req).await.expect_err("should fail");
    assert!(err.message().contains("both object_id"));
}
