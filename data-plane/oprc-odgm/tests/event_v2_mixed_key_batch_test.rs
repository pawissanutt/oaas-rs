use oprc_grpc::{
    DataTrigger, ObjData, ObjMeta, ObjectEvent, TriggerTarget, ValData, ValType,
};

mod common;
use common::EventTestContext;
use serial_test::serial;

fn make_target(fn_id: String) -> TriggerTarget {
    TriggerTarget::stateless("notification_service", 1, fn_id)
}

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn v2_batch_mixed_numeric_and_string_triggers()
-> Result<(), Box<dyn std::error::Error>> {
    // Ensure V2 is enabled and bridge disabled; enable trigger test path via subscriptions
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };

    let mut ctx = EventTestContext::new().await?;

    // We'll use two distinct function IDs so we can subscribe for each independently
    let fn_num = format!("on_data_create_num_{}", ctx.test_id);
    let fn_str = format!("on_data_create_str_{}", ctx.test_id);

    let sub_num = ctx.create_subscriber("on_data_create_num").await?;
    let sub_str = ctx.create_subscriber("on_data_create_str").await?;

    // Build ObjectEvent with both numeric and string-key DataTriggers (on_create)
    let mut ev = ObjectEvent::default();
    let mut dt_num = DataTrigger::default();
    dt_num.on_create.push(make_target(fn_num.clone()));
    ev.data_trigger.insert("1".to_string(), dt_num);

    let mut dt_str = DataTrigger::default();
    dt_str.on_create.push(make_target(fn_str.clone()));
    ev.data_trigger.insert("name".to_string(), dt_str);

    // Build object with both numeric and string entries
    let mut obj = ObjData::default();
    obj.metadata = Some(ObjMeta {
        cls_id: ctx.collection_name.clone(),
        partition_id: 1,
        object_id: Some("3001".to_string()),
    });
    obj.entries.insert(
        "1".to_string(),
        ValData {
            data: b"n1".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );
    obj.entries.insert(
        "name".into(),
        ValData {
            data: b"alice".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );
    obj.event = Some(ev);

    // First set should create both entries → two triggers should fire
    ctx.set_object(obj).await?;

    // Expect both invocations (order isn't guaranteed); wait independently
    let _s1 = ctx
        .wait_for_event(&sub_num, std::time::Duration::from_millis(2000))
        .await?;
    let _s2 = ctx
        .wait_for_event(&sub_str, std::time::Duration::from_millis(2000))
        .await?;

    ctx.shutdown().await;
    Ok(())
}
