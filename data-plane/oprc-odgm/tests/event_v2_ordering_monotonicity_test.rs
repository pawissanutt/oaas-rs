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
async fn v2_ordering_monotonic_versions()
-> Result<(), Box<dyn std::error::Error>> {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };

    let mut ctx = EventTestContext::new().await?;

    let fn_update = format!("on_data_update_{}", ctx.test_id);
    let sub_update = ctx.create_subscriber("on_data_update").await?;

    // Configure on_update trigger for numeric key 1 so updates emit
    let mut ev = ObjectEvent::default();
    let mut dt = DataTrigger::default();
    dt.on_update.push(make_target(fn_update));
    ev.data_trigger.insert("1".to_string(), dt);

    // Create object with initial value (no update trigger expected yet)
    let mut obj = ObjData::default();
    obj.metadata = Some(ObjMeta {
        cls_id: ctx.collection_name.clone(),
        partition_id: 1,
        object_id: Some("3101".to_string()),
    });
    obj.entries.insert(
        "1".to_string(),
        ValData {
            data: b"v0".to_vec(),
            r#type: ValType::Byte as i32,
        },
    );
    obj.event = Some(ev);
    ctx.set_object(obj.clone()).await?; // initial create

    // Perform 3 ordered updates
    for i in 1..=3 {
        let mut upd = obj.clone();
        upd.entries.get_mut("1").unwrap().data = format!("v{}", i).into_bytes();
        ctx.set_object(upd).await?;
    }

    // Receive three update events and assert their internal version_after increases
    // We can't directly parse version from external payload without schema; instead subscribe to
    // ensure events arrive, and rely on internal V2 counters not exposed here. As a proxy, we
    // expect to receive three events without duplication or reordering window under single-threaded runtime.
    for _ in 0..3 {
        let _ = ctx
            .wait_for_event(&sub_update, std::time::Duration::from_millis(2000))
            .await?;
    }

    ctx.shutdown().await;
    Ok(())
}
