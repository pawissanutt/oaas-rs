use oprc_grpc::{
    ObjData, SetObjectRequest, data_service_client::DataServiceClient,
};
use tokio::task;

mod common;
use common::{TestConfig, TestEnvironment};

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_set_same_string_id_is_idempotent_upsert() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");
    env.create_test_collection("coll")
        .await
        .expect("create collection");

    let set_req_template = SetObjectRequest {
        cls_id: "coll".into(),
        partition_id: 0,
        object_id: Some("user-42".into()),
        object: Some(ObjData {
            ..Default::default()
        }),
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
    let mut other = 0;
    for t in tasks {
        match t.await.unwrap() {
            Ok(_) => ok += 1,
            Err(status) => {
                eprintln!("unexpected error: {:?}", status);
                other += 1
            }
        }
    }
    assert_eq!(ok, 8, "all concurrent Set calls should succeed (upsert)");
    assert_eq!(other, 0, "no unexpected errors");
}
