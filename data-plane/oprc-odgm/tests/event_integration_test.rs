mod common;
use common::{TestConfig, TestEnvironment};
use oprc_pb::{
    data_service_client::DataServiceClient, DataTrigger, FuncTrigger, ObjData,
    ObjMeta, ObjectEvent, SetObjectRequest, SingleObjectRequest, TriggerTarget,
    ValData, ValType,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU16, Ordering},
    time::Duration,
};
use tracing::{debug, error, info, warn};
use zenoh::Session;

// Global port allocator to avoid conflicts between concurrent tests
static NEXT_PORT: AtomicU16 = AtomicU16::new(50600);

// Helper struct to encapsulate test environment and common operations
struct EventTestContext {
    session: Session,
    client: DataServiceClient<tonic::transport::Channel>,
    env: TestEnvironment,
    config: TestConfig,
    test_id: String,
    collection_name: String,
}

impl EventTestContext {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Generate unique identifiers for this test instance
        let test_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let collection_name = format!("test_collection_{}", test_id);

        info!(
            "Starting EventTestContext initialization with test_id: {}",
            test_id
        );

        // Allocate unique ports for this test instance
        let http_port = NEXT_PORT.fetch_add(3, Ordering::SeqCst);
        let zenoh_port = http_port + 1;
        let cluster_port = http_port + 2;

        info!(
            "Allocated ports - HTTP: {}, Zenoh: {}, Cluster: {}",
            http_port, zenoh_port, cluster_port
        );

        // Setup ODGM config with events enabled and unique ports
        debug!("Setting up ODGM config with unique ports");
        let mut config = TestConfig::new().await;
        config.odgm_config.events_enabled = true;
        config.odgm_config.http_port = http_port;

        info!(
            "ODGM config created with events enabled: {}, HTTP port: {}",
            config.odgm_config.events_enabled, config.odgm_config.http_port
        );

        // Start ODGM server
        info!("Starting ODGM server on port {}", http_port);
        let env = TestEnvironment::new(config.clone()).await;
        env.start_odgm().await.unwrap();
        info!("ODGM server started on port {}", http_port);

        // Create test collection with unique name
        debug!("Creating test collection: {}", collection_name);
        env.create_test_collection(&collection_name).await.unwrap();
        info!("Test collection '{}' created", collection_name);

        // Create gRPC client
        let client_addr =
            format!("http://127.0.0.1:{}", config.odgm_config.http_port);
        debug!("Connecting to gRPC client at: {}", client_addr);
        let client = DataServiceClient::connect(client_addr).await.unwrap();
        info!("gRPC client connected successfully");

        info!(
            "EventTestContext initialization completed for test_id: {}",
            test_id
        );
        let session = env.get_session().await;
        Ok(Self {
            session,
            client,
            env,
            config,
            test_id,
            collection_name,
        })
    }

    async fn create_subscriber(
        &self,
        event_type: &str,
    ) -> Result<
        zenoh::pubsub::Subscriber<
            zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>,
        >,
        Box<dyn std::error::Error>,
    > {
        let keyexpr = format!(
            "oprc/notification_service/1/async/{}_{}/*",
            event_type, self.test_id
        );
        info!("Creating subscriber for keyexpr: {}", keyexpr);
        debug!(
            "Subscribing to event type: {} for test_id: {}",
            event_type, self.test_id
        );

        let subscriber =
            self.session.declare_subscriber(keyexpr).await.unwrap();
        info!(
            "Successfully created subscriber for event type: {} (test_id: {})",
            event_type, self.test_id
        );
        Ok(subscriber)
    }

    fn create_data_trigger(
        &self,
        event_type: &str,
        fn_id: &str,
    ) -> DataTrigger {
        let unique_fn_id = format!("{}_{}", fn_id, self.test_id);
        debug!(
            "Creating data trigger for event_type: {}, fn_id: {}, test_id: {}",
            event_type, unique_fn_id, self.test_id
        );

        let mut data_trigger = DataTrigger::default();
        let target = TriggerTarget {
            cls_id: "notification_service".to_string(),
            partition_id: 1,
            object_id: None,
            fn_id: unique_fn_id.clone(),
            req_options: HashMap::new(),
        };

        match event_type {
            "create" => {
                data_trigger.on_create.push(target);
                debug!("Added on_create trigger for fn_id: {}", unique_fn_id);
            }
            "update" => {
                data_trigger.on_update.push(target);
                debug!("Added on_update trigger for fn_id: {}", unique_fn_id);
            }
            "delete" => {
                data_trigger.on_delete.push(target);
                debug!("Added on_delete trigger for fn_id: {}", unique_fn_id);
            }
            _ => {
                error!("Unknown event type: {}", event_type);
                panic!("Unknown event type: {}", event_type);
            }
        }

        data_trigger
    }

    fn create_func_trigger(
        &self,
        event_type: &str,
        fn_id: &str,
    ) -> FuncTrigger {
        let unique_fn_id = format!("{}_{}", fn_id, self.test_id);
        debug!(
            "Creating function trigger for event_type: {}, fn_id: {}, test_id: {}",
            event_type, unique_fn_id, self.test_id
        );

        let mut func_trigger = FuncTrigger::default();
        let target = TriggerTarget {
            cls_id: "notification_service".to_string(),
            partition_id: 1,
            object_id: None,
            fn_id: unique_fn_id.clone(),
            req_options: HashMap::new(),
        };

        match event_type {
            "complete" => {
                func_trigger.on_complete.push(target);
                debug!(
                    "Added on_complete function trigger for fn_id: {}",
                    unique_fn_id
                );
            }
            "error" => {
                func_trigger.on_error.push(target);
                debug!(
                    "Added on_error function trigger for fn_id: {}",
                    unique_fn_id
                );
            }
            _ => {
                error!("Unknown function event type: {}", event_type);
                panic!("Unknown function event type: {}", event_type);
            }
        }

        func_trigger
    }

    fn create_test_object(
        &self,
        object_id: u64,
        data: &[u8],
        object_event: Option<ObjectEvent>,
    ) -> ObjData {
        debug!(
            "Creating test object with id: {}, data_len: {} bytes, collection: {}",
            object_id,
            data.len(),
            self.collection_name
        );

        let mut entries = HashMap::new();
        entries.insert(
            1,
            ValData {
                data: data.to_vec(),
                r#type: ValType::Byte as i32,
            },
        );

        let obj_data = ObjData {
            metadata: Some(ObjMeta {
                cls_id: self.collection_name.clone(),
                partition_id: 1,
                object_id,
            }),
            entries,
            event: object_event.clone(),
        };

        if object_event.is_some() {
            debug!("Test object created with event triggers attached");
        } else {
            debug!("Test object created without event triggers");
        }

        obj_data
    }

    async fn set_object(
        &mut self,
        obj: ObjData,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ObjMeta {
            cls_id,
            partition_id,
            object_id,
        } = obj.metadata.as_ref().unwrap();
        let cls_id = cls_id.clone();
        let object_id = *object_id;
        let partition_id = *partition_id as i32;

        info!(
            "Setting object: cls_id={}, partition_id={}, object_id={}",
            cls_id.clone(),
            partition_id,
            object_id
        );

        let result = self
            .client
            .set(SetObjectRequest {
                cls_id: cls_id.clone(),
                partition_id: partition_id,
                object_id: object_id,
                object: Some(obj),
            })
            .await;

        match &result {
            Ok(_) => {
                info!("Successfully set object with id: {}", object_id)
            }
            Err(e) => error!(
                "Failed to set object with id: {}, error: {:?}",
                object_id, e
            ),
        }

        result?;
        Ok(())
    }

    async fn delete_object(
        &mut self,
        obj: &ObjData,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = obj.metadata.as_ref().unwrap();
        info!(
            "Deleting object: cls_id={}, partition_id={}, object_id={}",
            metadata.cls_id, metadata.partition_id, metadata.object_id
        );

        let result = self
            .client
            .delete(SingleObjectRequest {
                cls_id: metadata.cls_id.clone(),
                partition_id: metadata.partition_id,
                object_id: metadata.object_id,
            })
            .await;

        match &result {
            Ok(_) => info!(
                "Successfully deleted object with id: {}",
                metadata.object_id
            ),
            Err(e) => error!(
                "Failed to delete object with id: {}, error: {:?}",
                metadata.object_id, e
            ),
        }

        result?;
        Ok(())
    }

    async fn wait_for_event(
        &self,
        subscriber: &zenoh::pubsub::Subscriber<
            zenoh::handlers::FifoChannelHandler<zenoh::sample::Sample>,
        >,
        timeout_ms: u64,
        error_msg: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Waiting for event with timeout: {}ms", timeout_ms);
        // Await Zenoh event publication
        info!("Listening for Zenoh event publication...");
        let sample_result = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            subscriber.recv_async(),
        )
        .await;

        match sample_result {
            Ok(Ok(sample)) => {
                let payload = sample.payload();
                info!("✅ Received event payload: {} bytes", payload.len());
                debug!("Event sample key: {}", sample.key_expr());
                assert!(!payload.is_empty(), "Expected non-empty payload");
                Ok(())
            }
            Ok(Err(e)) => {
                error!("❌ Error receiving event: {:?}", e);
                Err(format!("Error receiving event: {:?}", e).into())
            }
            Err(_) => {
                warn!("⏰ Timeout waiting for event after {}ms", timeout_ms);
                Err(error_msg.to_string().into())
            }
        }
    }

    async fn shutdown(self) {
        info!(
            "Shutting down EventTestContext for test_id: {}",
            self.test_id
        );
        self.env.shutdown().await;
        info!(
            "EventTestContext shutdown completed for test_id: {}",
            self.test_id
        );
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_data_create_event_integration(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;

    info!("🧪 Starting test_data_create_event_integration");
    let subscriber = ctx.create_subscriber("on_data_create").await?;
    info!("Events enabled: {}", ctx.config.odgm_config.events_enabled);

    // Create object with on_create data trigger
    debug!("Creating object with on_create data trigger");
    let mut object_event = ObjectEvent::default();
    let data_trigger = ctx.create_data_trigger("create", "on_data_create");
    object_event.data_trigger.insert(1, data_trigger);

    let obj = ctx.create_test_object(42, b"test_value", Some(object_event));

    ctx.set_object(obj).await?;
    info!("Object set via gRPC, waiting for event...");

    ctx.wait_for_event(&subscriber, 500, "Timeout waiting for event")
        .await?;
    info!("✅ test_data_create_event_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_data_update_event_integration(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;

    info!("🧪 Starting test_data_update_event_integration");
    let subscriber = ctx.create_subscriber("on_data_update").await?;

    // First, create an object without events
    info!("Creating initial object without events");
    let initial_obj = ctx.create_test_object(43, b"initial_value", None);
    ctx.set_object(initial_obj).await?;

    // Now update the object with on_update data trigger
    info!("Updating object with on_update data trigger");
    let mut object_event = ObjectEvent::default();
    let data_trigger = ctx.create_data_trigger("update", "on_data_update");
    object_event.data_trigger.insert(1, data_trigger);

    let mut updated_obj =
        ctx.create_test_object(43, b"value", Some(object_event));
    ctx.set_object(updated_obj.clone()).await?;
    updated_obj.entries.insert(
        1,
        ValData {
            data: b"updated_value".to_vec(),
            r#type: oprc_pb::ValType::Byte as i32,
        },
    );
    ctx.set_object(updated_obj.clone()).await?;

    ctx.wait_for_event(&subscriber, 5000, "Timeout waiting for update event")
        .await?;

    info!("✅ test_data_update_event_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_data_delete_event_integration(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;

    info!("🧪 Starting test_data_delete_event_integration");
    let subscriber = ctx.create_subscriber("on_data_delete").await?;

    // Create an object with on_delete data trigger
    info!("Creating object with on_delete data trigger");
    let mut object_event = ObjectEvent::default();
    let data_trigger = ctx.create_data_trigger("delete", "on_data_delete");
    object_event.data_trigger.insert(1, data_trigger);

    let obj = ctx.create_test_object(44, b"to_be_deleted", Some(object_event));

    // First create the object
    info!("Setting object before deletion");
    ctx.set_object(obj.clone()).await?;

    // Now delete the object to trigger the delete event
    info!("Deleting object to trigger delete event");
    ctx.delete_object(&obj).await?;

    ctx.wait_for_event(&subscriber, 500, "Timeout waiting for delete event")
        .await?;

    info!("✅ test_data_delete_event_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_function_complete_event_integration(
) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = EventTestContext::new().await?;

    info!("🧪 Starting test_function_complete_event_integration");
    let _subscriber = ctx.create_subscriber("on_func_complete").await?;

    // Create object with on_complete function trigger
    info!("Creating object with on_complete function trigger");
    let mut object_event = ObjectEvent::default();
    let func_trigger = ctx.create_func_trigger("complete", "on_func_complete");
    object_event
        .func_trigger
        .insert("test_function".to_string(), func_trigger);

    let _obj =
        ctx.create_test_object(45, b"function_test_value", Some(object_event));

    // Note: In a real scenario, we would need to invoke a function and have it complete
    // For this integration test, we're testing the event system setup
    // The actual function completion would be triggered by the function execution system
    info!("Note: Function completion events require actual function execution");

    // Wait a bit to ensure the object is created and event system is ready
    debug!("Waiting for event system to be ready");
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("✅ test_function_complete_event_integration setup completed");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_function_error_event_integration(
) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = EventTestContext::new().await?;

    info!("🧪 Starting test_function_error_event_integration");
    let _subscriber = ctx.create_subscriber("on_func_error").await?;

    // Create object with on_error function trigger
    info!("Creating object with on_error function trigger");
    let mut object_event = ObjectEvent::default();
    let func_trigger = ctx.create_func_trigger("error", "on_func_error");
    object_event
        .func_trigger
        .insert("test_function".to_string(), func_trigger);

    let _obj = ctx.create_test_object(
        46,
        b"function_error_test_value",
        Some(object_event),
    );

    // Note: Similar to the function complete test, in a real scenario we would:
    // 1. Invoke a function that will fail
    // 2. Wait for the function to error
    // 3. Verify the error event is triggered
    info!("Note: Function error events require actual function execution that fails");

    // Wait a bit to ensure the object is created and event system is ready
    debug!("Waiting for event system to be ready");
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("✅ test_function_error_event_integration setup completed");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
async fn test_multiple_event_types_integration(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;

    info!("🧪 Starting test_multiple_event_types_integration");

    // Subscribe to multiple event trigger publication keys
    info!("Creating subscribers for multiple event types");
    let create_subscriber = ctx.create_subscriber("on_data_create").await?;
    let update_subscriber = ctx.create_subscriber("on_data_update").await?;
    let delete_subscriber = ctx.create_subscriber("on_data_delete").await?;

    // Build object with multiple data triggers
    info!("Building object with multiple data triggers");
    let mut object_event = ObjectEvent::default();
    let mut data_trigger = DataTrigger::default();

    // Add all three data event triggers with unique function IDs
    data_trigger.on_create.push(TriggerTarget {
        cls_id: "notification_service".to_string(),
        partition_id: 1,
        object_id: None,
        fn_id: format!("on_data_create_{}", ctx.test_id),
        req_options: HashMap::new(),
    });
    data_trigger.on_update.push(TriggerTarget {
        cls_id: "notification_service".to_string(),
        partition_id: 1,
        object_id: None,
        fn_id: format!("on_data_update_{}", ctx.test_id),
        req_options: HashMap::new(),
    });
    data_trigger.on_delete.push(TriggerTarget {
        cls_id: "notification_service".to_string(),
        partition_id: 1,
        object_id: None,
        fn_id: format!("on_data_delete_{}", ctx.test_id),
        req_options: HashMap::new(),
    });

    object_event.data_trigger.insert(1, data_trigger);

    let obj = ctx.create_test_object(
        47,
        b"multi_event_test_value",
        Some(object_event.clone()),
    );

    // Test CREATE event
    info!("Testing CREATE event");
    ctx.set_object(obj.clone()).await?;
    ctx.wait_for_event(
        &create_subscriber,
        500,
        "Timeout waiting for create event",
    )
    .await?;

    // Test UPDATE event
    info!("Testing UPDATE event");
    let updated_obj = ctx.create_test_object(
        47,
        b"updated_multi_event_value",
        Some(object_event),
    );
    ctx.set_object(updated_obj).await?;
    ctx.wait_for_event(
        &update_subscriber,
        500,
        "Timeout waiting for update event",
    )
    .await?;

    // Test DELETE event
    info!("Testing DELETE event");
    ctx.delete_object(&obj).await?;
    ctx.wait_for_event(
        &delete_subscriber,
        500,
        "Timeout waiting for delete event",
    )
    .await?;

    info!("✅ test_multiple_event_types_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}
