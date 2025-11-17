mod common;
use common::EventTestContext;
use oprc_grpc::{DataTrigger, ObjectEvent, TriggerTarget, ValData};
use serial_test::serial;
use std::time::Duration;
use tracing::{debug, info};

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_data_create_event_integration()
-> Result<(), Box<dyn std::error::Error>> {
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

    ctx.wait_for_event(&subscriber, Duration::from_millis(500))
        .await?;
    info!("✅ test_data_create_event_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_data_update_event_integration()
-> Result<(), Box<dyn std::error::Error>> {
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
            r#type: oprc_grpc::ValType::Byte as i32,
        },
    );
    ctx.set_object(updated_obj.clone()).await?;

    ctx.wait_for_event(&subscriber, Duration::from_millis(5000))
        .await?;

    info!("✅ test_data_update_event_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_data_delete_event_integration()
-> Result<(), Box<dyn std::error::Error>> {
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

    ctx.wait_for_event(&subscriber, Duration::from_millis(500))
        .await?;

    info!("✅ test_data_delete_event_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_function_complete_event_integration()
-> Result<(), Box<dyn std::error::Error>> {
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
#[serial]
async fn test_function_error_event_integration()
-> Result<(), Box<dyn std::error::Error>> {
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
    info!(
        "Note: Function error events require actual function execution that fails"
    );

    // Wait a bit to ensure the object is created and event system is ready
    debug!("Waiting for event system to be ready");
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("✅ test_function_error_event_integration setup completed");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_multiple_event_types_integration()
-> Result<(), Box<dyn std::error::Error>> {
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
    data_trigger.on_create.push(TriggerTarget::stateless(
        "notification_service",
        1,
        format!("on_data_create_{}", ctx.test_id),
    ));
    data_trigger.on_update.push(TriggerTarget::stateless(
        "notification_service",
        1,
        format!("on_data_update_{}", ctx.test_id),
    ));
    data_trigger.on_delete.push(TriggerTarget::stateless(
        "notification_service",
        1,
        format!("on_data_delete_{}", ctx.test_id),
    ));

    object_event.data_trigger.insert(1, data_trigger);

    let obj = ctx.create_test_object(
        47,
        b"multi_event_test_value",
        Some(object_event.clone()),
    );

    // Test CREATE event
    info!("Testing CREATE event");
    ctx.set_object(obj.clone()).await?;
    ctx.wait_for_event(&create_subscriber, Duration::from_millis(500))
        .await?;

    // Test UPDATE event
    info!("Testing UPDATE event");
    let updated_obj = ctx.create_test_object(
        47,
        b"updated_multi_event_value",
        Some(object_event),
    );
    ctx.set_object(updated_obj).await?;
    ctx.wait_for_event(&update_subscriber, Duration::from_millis(500))
        .await?;

    // Test DELETE event
    info!("Testing DELETE event");
    ctx.delete_object(&obj).await?;
    ctx.wait_for_event(&delete_subscriber, Duration::from_millis(500))
        .await?;

    info!("✅ test_multiple_event_types_integration completed successfully");
    ctx.shutdown().await;
    Ok(())
}

// Tests with different shard types
#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_data_create_event_with_mst_shard()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new_with_shard_type("mst").await?;

    info!("🧪 Starting test_data_create_event_with_mst_shard");
    let subscriber = ctx.create_subscriber("on_data_create_mst").await?;

    // Create object with on_create data trigger for MST shard
    debug!("Creating object with on_create data trigger for MST shard");
    let mut object_event = ObjectEvent::default();
    let data_trigger = ctx.create_data_trigger("create", "on_data_create_mst");
    object_event.data_trigger.insert(1, data_trigger);

    let obj = ctx.create_test_object(48, b"mst_test_value", Some(object_event));

    ctx.set_object(obj).await?;
    info!("Object set via gRPC on MST shard, waiting for event...");

    ctx.wait_for_event(&subscriber, Duration::from_millis(1000))
        .await?;
    info!("✅ test_data_create_event_with_mst_shard completed successfully");
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_data_create_event_with_raft_shard()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new_with_shard_type("raft").await?;

    info!("🧪 Starting test_data_create_event_with_raft_shard");
    let subscriber = ctx.create_subscriber("on_data_create_raft").await?;

    // Create object with on_create data trigger for Raft shard
    debug!("Creating object with on_create data trigger for Raft shard");
    let mut object_event = ObjectEvent::default();
    let data_trigger = ctx.create_data_trigger("create", "on_data_create_raft");
    object_event.data_trigger.insert(1, data_trigger);

    let obj =
        ctx.create_test_object(49, b"raft_test_value", Some(object_event));

    ctx.set_object(obj).await?;
    info!("Object set via gRPC on Raft shard, waiting for event...");

    ctx.wait_for_event(&subscriber, Duration::from_millis(2000))
        .await?;
    info!("✅ test_data_create_event_with_raft_shard completed successfully");
    ctx.shutdown().await;
    Ok(())
}
