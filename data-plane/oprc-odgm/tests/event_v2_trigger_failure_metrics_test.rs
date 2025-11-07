use std::collections::HashMap;

use oprc_grpc::{
    DataTrigger, ObjData, ObjMeta, ObjectEvent, TriggerTarget, ValData, ValType,
};

mod common;
use common::EventTestContext;
use serial_test::serial;

#[test_log::test(tokio::test(flavor = "multi_thread", worker_threads = 1))]
#[serial]
async fn v2_trigger_publish_failure_increments_metric()
-> Result<(), Box<dyn std::error::Error>> {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    // Force publish failure inside TriggerProcessor
    unsafe { std::env::set_var("ODGM_FORCE_TRIGGER_PUBLISH_FAIL", "1") };

    let mut ctx = EventTestContext::new().await?;

    // Configure an on_create trigger to ensure dispatch path runs
    let mut ev = ObjectEvent::default();
    let mut dt = DataTrigger::default();
    dt.on_create.push(TriggerTarget {
        cls_id: "notification_service".into(),
        partition_id: 1,
        object_id: None,
        fn_id: format!("on_data_create_{}", ctx.test_id),
        req_options: HashMap::new(),
    });
    ev.data_trigger.insert(1, dt);

    let mut obj = ObjData::default();
    obj.metadata = Some(ObjMeta {
        cls_id: ctx.collection_name.clone(),
        partition_id: 1,
        object_id: 3201,
        object_id_str: None,
    });
    obj.entries.insert(
        1,
        ValData {
            data: b"v1".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );
    obj.event = Some(ev);

    // Perform set to trigger publish attempt (which will fail)
    ctx.set_object(obj).await?;

    // Give the async path a brief moment to execute
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Read failures counter via public helper
    let failures = oprc_odgm::events::processor::trigger_failures_total();
    assert!(
        failures >= 1,
        "expected at least one trigger publish failure, got {}",
        failures
    );

    // Cleanup: unset the env var so it doesn't affect other tests
    unsafe { std::env::remove_var("ODGM_FORCE_TRIGGER_PUBLISH_FAIL") };

    ctx.shutdown().await;
    Ok(())
}
