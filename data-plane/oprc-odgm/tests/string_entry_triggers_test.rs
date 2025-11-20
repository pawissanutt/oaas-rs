use oprc_grpc::{
    DataTrigger, ObjData, ObjMeta, ObjectEvent, TriggerTarget, ValData, ValType,
};

// Reuse existing common test utilities if available
mod common;
use common::EventTestContext;
use serial_test::serial;

// Helper to build a string-key object with triggers
fn build_obj_with_str_entry_and_triggers(
    cls_id: &str,
    partition_id: u32,
    object_id: &str,
    key: &str,
    val: &[u8],
    trigger: Option<DataTrigger>,
) -> ObjData {
    let mut event = ObjectEvent::default();
    if let Some(dt) = trigger {
        event.data_trigger.insert(key.to_string(), dt);
    }
    let mut data = ObjData::default();
    data.entries.insert(
        key.to_string(),
        ValData {
            data: val.to_vec(),
            r#type: ValType::Byte as i32,
        },
    );
    data.metadata = Some(ObjMeta {
        cls_id: cls_id.to_string(),
        partition_id: partition_id,
        object_id: Some(object_id.to_string()),
    });
    data.event = Some(event);
    data
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_string_entry_create_trigger()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;
    let create_sub = ctx.create_subscriber("on_data_create").await?; // existing topic reused
    // only on_create trigger needed for this test
    let mut dt = DataTrigger::default();
    dt.on_create.push(TriggerTarget::stateless(
        "notification_service",
        1,
        format!("on_data_create_{}", ctx.test_id),
    ));
    let obj = build_obj_with_str_entry_and_triggers(
        "test_collection",
        1,
        "2001",
        "status",
        b"new",
        Some(dt),
    );
    ctx.set_object_flex(obj).await?; // first create
    ctx.wait_for_event(&create_sub, std::time::Duration::from_millis(1000))
        .await?;
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_string_entry_update_trigger()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;
    let update_sub = ctx.create_subscriber("on_data_update").await?;
    // create with only update trigger (skip create to avoid expecting create event)
    let mut dt = DataTrigger::default();
    dt.on_update.push(TriggerTarget::stateless(
        "notification_service",
        1,
        format!("on_data_update_{}", ctx.test_id),
    ));
    let mut obj = build_obj_with_str_entry_and_triggers(
        "test_collection",
        1,
        "2002",
        "mode",
        b"cold",
        Some(dt),
    );
    ctx.set_object_flex(obj.clone()).await?; // initial set
    // simulate update: modify value then set again (numeric object id => upsert semantics)
    if let Some(v) = obj.entries.get_mut("mode") {
        v.data = b"hot".to_vec();
    }
    ctx.set_object_flex(obj).await?; // second call should trigger update
    ctx.wait_for_event(&update_sub, std::time::Duration::from_millis(1500))
        .await?;
    ctx.shutdown().await;
    Ok(())
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn test_string_entry_delete_trigger()
-> Result<(), Box<dyn std::error::Error>> {
    let mut ctx = EventTestContext::new().await?;
    let delete_sub = ctx.create_subscriber("on_data_delete").await?;
    // delete trigger only
    let mut dt = DataTrigger::default();
    dt.on_delete.push(TriggerTarget::stateless(
        "notification_service",
        1,
        format!("on_data_delete_{}", ctx.test_id),
    ));
    let obj = build_obj_with_str_entry_and_triggers(
        "test_collection",
        1,
        "2003",
        "phase",
        b"start",
        Some(dt),
    );
    ctx.set_object_flex(obj.clone()).await?; // initial create (no delete trigger occurs yet)
    // delete entire object to trigger per-entry delete for string key
    ctx.delete_object(&obj).await?;
    ctx.wait_for_event(&delete_sub, std::time::Duration::from_millis(2000))
        .await?;
    ctx.shutdown().await;
    Ok(())
}
