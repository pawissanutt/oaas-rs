use oprc_zrpc::bincode::BincodeZrpcType;
use oprc_zrpc::server::ServerConfig;
use oprc_zrpc::ZrpcClient;
use oprc_zrpc::ZrpcError;
use oprc_zrpc::ZrpcServiceHander;
use tracing::info;
use tracing_test::traced_test;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct InputMsg(i64);

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct OutputMsg(u64);

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct MyError(String);

struct TestHandler;
type TypeConf = BincodeZrpcType<InputMsg, OutputMsg, MyError>;

#[async_trait::async_trait]
impl ZrpcServiceHander<TypeConf> for TestHandler {
    async fn handle(&self, req: InputMsg) -> Result<OutputMsg, MyError> {
        info!("receive {:?}", req);
        let num = req.0;
        if num > 0 {
            Ok(OutputMsg((num * 2) as u64))
        } else {
            Err(MyError("num should be more than 0".into()))
        }
    }
}

type TestClient = ZrpcClient<TypeConf>;

type ConcurrentService = oprc_zrpc::server::ZrpcService<TestHandler, TypeConf>;

#[traced_test]
#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_service() {
    info!("start");
    let z_session = zenoh::open(zenoh::Config::default()).await.unwrap();
    let handler = TestHandler;
    let config = ServerConfig {
        service_id: "test/**".into(),
        concurrency: 4,
        bound_channel: 0,
        accept_subfix: false,
        ..Default::default()
    };
    let mut service =
        ConcurrentService::new(z_session.clone(), config, handler);
    let _service = service.start().await.unwrap();

    let z_session_2 = zenoh::open(zenoh::Config::default()).await.unwrap();
    let client = TestClient::new("test".into(), z_session_2.clone()).await;
    for i in 1..10 {
        info!("call rpc = {}", i);
        let out = client
            .call(&InputMsg(i))
            .await
            .expect("return should not error");
        info!("return output = {}", out.0);
        assert!(out.0 == (i * 2) as u64);
    }

    info!("call rpc = -2");
    let out = client
        .call(&InputMsg(-2))
        .await
        .expect_err("return should be error");
    info!("return err = {:?}", out);
    if let ZrpcError::AppError(_) = out {
    } else {
        panic!("expect error")
    }

    info!("closed z_session");
}
