use oprc_zrpc::ZrpcClient;
use oprc_zrpc::ZrpcError;
use oprc_zrpc::ZrpcServiceHander;
use oprc_zrpc::bincode::BincodeZrpcType;
use oprc_zrpc::server::ServerConfig;
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

    // Find a free port
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let endpoint = format!("tcp/127.0.0.1:{}", port);
    info!("Using endpoint: {}", endpoint);

    let mut config1 = zenoh::Config::default();
    config1
        .insert_json5("listen/endpoints", &format!(r#"["{}"]"#, endpoint))
        .unwrap();

    let z_session = zenoh::open(config1).await.unwrap();
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

    let mut config2 = zenoh::Config::default();
    config2
        .insert_json5("connect/endpoints", &format!(r#"["{}"]"#, endpoint))
        .unwrap();

    let z_session_2 = zenoh::open(config2).await.unwrap();
    let client = TestClient::new("test".into(), z_session_2.clone()).await;

    // Wait for discovery
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

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
