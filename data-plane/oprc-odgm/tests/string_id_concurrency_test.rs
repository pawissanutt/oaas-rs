use oprc_grpc::{
    ObjData, SetObjectRequest, SingleObjectRequest,
    data_service_client::DataServiceClient,
};
use tokio::task;
use tonic::transport::Channel;

mod common;
use common::{TestConfig, TestEnvironment};

async fn make_client(env: &TestEnvironment) -> DataServiceClient<Channel> {
    let addr = format!("http://127.0.0.1:{}", env.config.odgm_config.http_port);
    DataServiceClient::connect(addr).await.expect("connect")
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_set_same_string_id_only_one_succeeds() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("coll")
        .await
        .expect("create collection");

    let set_req_template = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: 0,
        object: Some(ObjData {
            ..Default::default()
        }),
        object_id_str: Some("user-42".into()),
    };

    let port = env.config.odgm_config.http_port;
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let req = set_req_template.clone();
            let addr = format!("http://127.0.0.1:{}", port);
            task::spawn(async move {
                let mut client =
                    DataServiceClient::connect(addr).await.expect("connect");
                client.set(req).await
            })
        })
        .collect();

    let mut ok = 0;
    let mut already = 0;
    let mut other = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(status) => {
                if status.code() == tonic::Code::AlreadyExists {
                    already += 1
                } else {
                    other += 1
                }
            }
        }
    }
    assert!(ok >= 1, "at least one Set should succeed");
    assert!(
        already >= 1,
        "at least one AlreadyExists expected under race"
    );
    assert_eq!(already + ok, 8, "all results accounted for");
    assert_eq!(other, 0, "no unexpected errors");
}

#[tokio::test(flavor = "multi_thread")]
async fn numeric_and_string_with_same_digits_do_not_conflict() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("coll")
        .await
        .expect("create collection");
    let mut client = make_client(&env).await;

    // Numeric create
    let req_num = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: 123,
        object: Some(ObjData {
            ..Default::default()
        }),
        object_id_str: None,
    };
    client.set(req_num).await.expect("set numeric");
    // String create with same digits
    let req_str = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: 0,
        object: Some(ObjData {
            ..Default::default()
        }),
        object_id_str: Some("123".into()),
    };
    client.set(req_str).await.expect("set string");

    let resp_num = client
        .get(SingleObjectRequest {
            cls_id: "coll".into(),
            partition_id: 0,
            object_id: 123,
            object_id_str: None,
        })
        .await
        .expect("get numeric");
    assert!(resp_num.into_inner().obj.is_some());
    let resp_str = client
        .get(SingleObjectRequest {
            cls_id: "coll".into(),
            partition_id: 0,
            object_id: 0,
            object_id_str: Some("123".into()),
        })
        .await
        .expect("get string");
    assert!(resp_str.into_inner().obj.is_some());
}
