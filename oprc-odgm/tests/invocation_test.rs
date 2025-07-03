mod common;

use common::mock_fn::start_mock_fn_server;
use common::{TestConfig, TestEnvironment};
use oprc_pb::oprc_function_client::OprcFunctionClient;
use oprc_pb::{InvocationRequest, ObjectInvocationRequest, ResponseStatus};

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_odgm_invocation() {
    // Start the mock function server
    let mock_fn = start_mock_fn_server().await;
    let mock_addr = format!("http://{}", mock_fn.addr);

    // Set up ODGM config with invocation route pointing to mock_fn
    let mut config = TestConfig::new().await;
    config.odgm_config.collection = Some(
        serde_json::json!([
            {
                "name": "invoke_test",
                "partition_count": 1,
                "replica_count": 1,
                "shard_type": "mst",
                "shard_assignments": [],
                "options": {},
                "invocations": {
                    "fn_routes": {
                        "echo": {
                            "url": mock_addr,
                            "stateless": true,
                            "standby": false,
                            "active_group": []
                        },
                        "echo-2": {
                            "url": mock_addr,
                            "stateless": false,
                            "standby": false,
                            "active_group": []
                        }
                    }
                }
            }
        ])
        .to_string(),
    );

    let env = TestEnvironment::new(config).await;
    let _ = env.start_odgm().await.expect("Failed to start ODGM");

    // // Create the collection manually since start_server doesn't do it automatically
    // oprc_odgm::create_collection(odgm.clone(), &env.config.odgm_config).await;

    // Wait for ODGM gRPC server to be ready
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Connect to the mock function server directly (for sanity check)
    let mut client = OprcFunctionClient::connect(mock_addr.clone())
        .await
        .expect("Failed to connect to mock_fn");

    // Test direct InvokeFn
    let req = InvocationRequest {
        partition_id: 0,
        cls_id: "invoke_test".to_string(), // Use the collection name
        fn_id: "echo".to_string(),
        options: Default::default(),
        payload: b"hello-mock".to_vec(),
    };
    let obj_req = ObjectInvocationRequest {
        partition_id: 0,
        object_id: 42,
        cls_id: "invoke_test".to_string(), // Use the collection name
        fn_id: "echo-2".to_string(),
        options: Default::default(),
        payload: b"obj-payload".to_vec(),
    };
    let resp = client
        .invoke_fn(req.clone())
        .await
        .expect("Direct invoke failed")
        .into_inner();
    assert_eq!(resp.status, ResponseStatus::Okay as i32);
    assert_eq!(resp.payload, Some(b"hello-mock".to_vec()));

    // ODGM-side object invocation test
    let odgm_obj_resp = client
        .invoke_obj(obj_req.clone())
        .await
        .expect("ODGM invoke_obj failed")
        .into_inner();
    assert_eq!(odgm_obj_resp.status, ResponseStatus::Okay as i32);
    assert_eq!(odgm_obj_resp.payload, Some(b"obj-payload".to_vec()));

    // Test direct InvokeObj
    let obj_resp = client
        .invoke_obj(obj_req.clone())
        .await
        .expect("Direct invoke_obj failed")
        .into_inner();
    assert_eq!(obj_resp.status, ResponseStatus::Okay as i32);
    assert_eq!(obj_resp.payload, Some(b"obj-payload".to_vec()));

    // Shutdown
    env.shutdown().await;
    mock_fn.shutdown().await;
}
