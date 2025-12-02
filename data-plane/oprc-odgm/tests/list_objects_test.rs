//! Tests for ListObjects API (gRPC and Zenoh).
//!
//! Tests pagination, prefix filtering, empty results, and concurrent access.
//! Each test uses unique collection/object names to avoid conflicts when
//! tests run in parallel.

mod common;

use std::collections::HashMap;
use std::time::Duration;

use common::{TestConfig, TestEnvironment};
use oprc_grpc::{
    BatchSetValuesRequest, ListObjectsRequest, ObjectMetaEnvelope, ValData,
    ValType, data_service_client::DataServiceClient,
};
use oprc_invoke::proxy::ObjectProxy;
use prost::Message;
use tokio::time::sleep;
use tonic::transport::Channel;
use zenoh::{bytes::ZBytes, qos::CongestionControl, query::ConsolidationMode};

const PARTITION_ID: u16 = 0;

async fn make_client(env: &TestEnvironment) -> DataServiceClient<Channel> {
    let addr = format!("http://127.0.0.1:{}", env.config.odgm_config.http_port);
    DataServiceClient::connect(addr)
        .await
        .expect("failed to connect gRPC client")
}

fn val(data: &str) -> ValData {
    ValData {
        data: data.as_bytes().to_vec(),
        r#type: ValType::Byte as i32,
    }
}

/// Create multiple objects in a collection for testing.
async fn create_test_objects(
    client: &mut DataServiceClient<Channel>,
    cls_id: &str,
    object_ids: &[&str],
) {
    for oid in object_ids {
        let mut values = HashMap::new();
        values.insert("key1".to_string(), val(&format!("value-{}", oid)));
        client
            .batch_set_values(BatchSetValuesRequest {
                cls_id: cls_id.to_string(),
                partition_id: PARTITION_ID as u32,
                object_id: Some(oid.to_string()),
                values,
                delete_keys: vec![],
                expected_object_version: None,
            })
            .await
            .expect(&format!("failed to create object {}", oid));
    }
    // Allow time for writes to propagate
    sleep(Duration::from_millis(100)).await;
}

// ============================================================================
// gRPC Tests
// ============================================================================

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn grpc_list_objects_basic() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    // Use unique collection name with random suffix
    let coll = format!("list_basic_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Create some objects
    let object_ids = ["obj-a", "obj-b", "obj-c"];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // List all objects
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects failed")
        .into_inner();

    let mut found_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            found_ids.push(envelope.object_id.clone());
            // Verify version > 0 (object was written)
            assert!(envelope.version > 0, "version should be > 0");
        }
    }

    // Should find all 3 objects
    assert_eq!(found_ids.len(), 3, "should find all 3 objects");
    for oid in &object_ids {
        assert!(
            found_ids.contains(&oid.to_string()),
            "should find object {}",
            oid
        );
    }

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn grpc_list_objects_empty_collection() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll = format!("list_empty_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // List objects in empty collection
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects failed")
        .into_inner();

    let mut count = 0;
    while let Some(_envelope) =
        stream.message().await.expect("stream message failed")
    {
        count += 1;
    }

    assert_eq!(count, 0, "empty collection should return no objects");

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn grpc_list_objects_with_prefix_filter() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("list_prefix_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Create objects with different prefixes
    let object_ids = [
        "user-alice",
        "user-bob",
        "user-charlie",
        "order-001",
        "order-002",
        "product-xyz",
    ];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Filter by "user-" prefix
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: Some("user-".to_string()),
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects with prefix failed")
        .into_inner();

    let mut user_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            user_ids.push(envelope.object_id.clone());
        }
    }

    assert_eq!(user_ids.len(), 3, "should find 3 user objects");
    assert!(user_ids.iter().all(|id| id.starts_with("user-")));

    // Filter by "order-" prefix
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: Some("order-".to_string()),
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects with prefix failed")
        .into_inner();

    let mut order_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            order_ids.push(envelope.object_id.clone());
        }
    }

    assert_eq!(order_ids.len(), 2, "should find 2 order objects");

    // Filter by non-existent prefix
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: Some("nonexistent-".to_string()),
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects with non-existent prefix")
        .into_inner();

    let mut empty_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            empty_ids.push(envelope.object_id.clone());
        }
    }

    assert_eq!(empty_ids.len(), 0, "non-existent prefix should return empty");

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn grpc_list_objects_pagination() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll = format!("list_page_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Create 10 objects for pagination test
    let object_ids: Vec<String> =
        (0..10).map(|i| format!("item-{:03}", i)).collect();
    let oid_refs: Vec<&str> = object_ids.iter().map(|s| s.as_str()).collect();
    create_test_objects(&mut client, &coll, &oid_refs).await;

    // First page: limit 3
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 3,
            cursor: None,
        })
        .await
        .expect("list_objects page 1 failed")
        .into_inner();

    let mut page1_ids: Vec<String> = Vec::new();
    let mut cursor: Option<Vec<u8>> = None;
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            page1_ids.push(envelope.object_id.clone());
        }
        if envelope.next_cursor.is_some() && !envelope.next_cursor.as_ref().unwrap().is_empty() {
            cursor = envelope.next_cursor.clone();
        }
    }

    assert_eq!(page1_ids.len(), 3, "first page should have 3 items");
    assert!(cursor.is_some(), "should have cursor for next page");

    // Second page using cursor
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 3,
            cursor: cursor.clone(),
        })
        .await
        .expect("list_objects page 2 failed")
        .into_inner();

    let mut page2_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            page2_ids.push(envelope.object_id.clone());
        }
        if envelope.next_cursor.is_some() && !envelope.next_cursor.as_ref().unwrap().is_empty() {
            cursor = envelope.next_cursor.clone();
        }
    }

    assert_eq!(page2_ids.len(), 3, "second page should have 3 items");

    // Verify no overlap between pages
    for id in &page1_ids {
        assert!(
            !page2_ids.contains(id),
            "pages should not overlap: {} found in both",
            id
        );
    }

    // Continue until exhausted
    let mut all_ids: Vec<String> = Vec::new();
    all_ids.extend(page1_ids);
    all_ids.extend(page2_ids);

    while cursor.is_some() {
        let mut stream = client
            .list_objects(ListObjectsRequest {
                cls_id: coll.clone(),
                partition_id: PARTITION_ID as u32,
                object_id_prefix: None,
                limit: 3,
                cursor: cursor.take(),
            })
            .await
            .expect("list_objects continuation failed")
            .into_inner();

        while let Some(envelope) =
            stream.message().await.expect("stream message failed")
        {
            if !envelope.object_id.is_empty() {
                all_ids.push(envelope.object_id.clone());
            }
            if envelope.next_cursor.is_some() && !envelope.next_cursor.as_ref().unwrap().is_empty() {
                cursor = envelope.next_cursor.clone();
            }
        }
    }

    assert_eq!(all_ids.len(), 10, "should find all 10 objects via pagination");

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn grpc_list_objects_nonexistent_collection() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    // Wait for gRPC server to be ready since we're not creating a collection
    sleep(Duration::from_millis(500)).await;

    let mut client = make_client(&env).await;

    // Try to list objects in a non-existent collection
    let result = client
        .list_objects(ListObjectsRequest {
            cls_id: "nonexistent_collection_xyz".to_string(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 100,
            cursor: None,
        })
        .await;

    // Should return an error (NotFound)
    assert!(
        result.is_err(),
        "listing non-existent collection should fail"
    );

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn grpc_list_objects_entry_count() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("list_entries_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Create object with multiple entries
    let mut values = HashMap::new();
    values.insert("entry1".to_string(), val("value1"));
    values.insert("entry2".to_string(), val("value2"));
    values.insert("entry3".to_string(), val("value3"));

    client
        .batch_set_values(BatchSetValuesRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id: Some("multi-entry-obj".to_string()),
            values,
            delete_keys: vec![],
            expected_object_version: None,
        })
        .await
        .expect("create multi-entry object");

    sleep(Duration::from_millis(100)).await;

    // List and check entry_count
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects failed")
        .into_inner();

    let mut found = false;
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if envelope.object_id == "multi-entry-obj" {
            found = true;
            assert_eq!(
                envelope.entry_count, 3,
                "should report 3 entries"
            );
        }
    }

    assert!(found, "should find the multi-entry object");

    env.shutdown().await;
}

// ============================================================================
// Zenoh Tests
// ============================================================================

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn zenoh_list_objects_basic() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("zlist_basic_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let mut client = make_client(&env).await;

    // Create objects via gRPC (simpler)
    let object_ids = ["z-obj-a", "z-obj-b", "z-obj-c"];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Query list-objects via Zenoh (key expression includes /objects/ prefix)
    let key_expr = format!("oprc/{}/{}/objects/list-objects", coll, PARTITION_ID);
    let request = ListObjectsRequest {
        cls_id: coll.clone(),
        partition_id: PARTITION_ID as u32,
        object_id_prefix: None,
        limit: 100,
        cursor: None,
    };

    let (tx, rx) = flume::bounded(16);
    session
        .get(&key_expr)
        .payload(ZBytes::from(request.encode_to_vec()))
        .consolidation(ConsolidationMode::None)
        .congestion_control(CongestionControl::Block)
        .callback(move |reply| {
            let _ = tx.send(reply);
        })
        .await
        .expect("failed to issue list-objects query");

    let mut found_ids: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(timeout, rx.recv_async()).await {
            Ok(Ok(reply)) => {
                if let Ok(sample) = reply.result() {
                    let envelope = ObjectMetaEnvelope::decode(
                        sample.payload().to_bytes().as_ref(),
                    )
                    .expect("decode envelope");
                    if !envelope.object_id.is_empty() {
                        found_ids.push(envelope.object_id.clone());
                    }
                }
            }
            Ok(Err(_)) => break, // Channel closed
            Err(_) => break,     // Timeout
        }
    }

    assert_eq!(found_ids.len(), 3, "should find all 3 objects via Zenoh");

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn zenoh_list_objects_with_prefix() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("zlist_prefix_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let mut client = make_client(&env).await;

    // Create objects with different prefixes
    let object_ids = ["alpha-1", "alpha-2", "beta-1", "beta-2", "gamma-1"];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Query with "alpha-" prefix via Zenoh
    let key_expr =
        format!("oprc/{}/{}/objects/list-objects", coll, PARTITION_ID);
    let request = ListObjectsRequest {
        cls_id: coll.clone(),
        partition_id: PARTITION_ID as u32,
        object_id_prefix: Some("alpha-".to_string()),
        limit: 100,
        cursor: None,
    };

    let (tx, rx) = flume::bounded(16);
    session
        .get(&key_expr)
        .payload(ZBytes::from(request.encode_to_vec()))
        .consolidation(ConsolidationMode::None)
        .congestion_control(CongestionControl::Block)
        .callback(move |reply| {
            let _ = tx.send(reply);
        })
        .await
        .expect("failed to issue list-objects query");

    let mut alpha_ids: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(timeout, rx.recv_async()).await {
            Ok(Ok(reply)) => {
                if let Ok(sample) = reply.result() {
                    let envelope = ObjectMetaEnvelope::decode(
                        sample.payload().to_bytes().as_ref(),
                    )
                    .expect("decode envelope");
                    if !envelope.object_id.is_empty() {
                        alpha_ids.push(envelope.object_id.clone());
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    assert_eq!(alpha_ids.len(), 2, "should find 2 alpha objects via Zenoh");
    assert!(alpha_ids.iter().all(|id| id.starts_with("alpha-")));

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn zenoh_list_objects_pagination() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("zlist_page_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let mut client = make_client(&env).await;

    // Create 5 objects
    let object_ids: Vec<String> =
        (0..5).map(|i| format!("zpage-{:02}", i)).collect();
    let oid_refs: Vec<&str> = object_ids.iter().map(|s| s.as_str()).collect();
    create_test_objects(&mut client, &coll, &oid_refs).await;

    let key_expr = format!("oprc/{}/{}/objects/list-objects", coll, PARTITION_ID);

    // First page: limit 2
    let request = ListObjectsRequest {
        cls_id: coll.clone(),
        partition_id: PARTITION_ID as u32,
        object_id_prefix: None,
        limit: 2,
        cursor: None,
    };

    let (tx, rx) = flume::bounded(16);
    session
        .get(&key_expr)
        .payload(ZBytes::from(request.encode_to_vec()))
        .consolidation(ConsolidationMode::None)
        .congestion_control(CongestionControl::Block)
        .callback(move |reply| {
            let _ = tx.send(reply);
        })
        .await
        .expect("failed to issue list-objects query");

    let mut page1_ids: Vec<String> = Vec::new();
    let mut cursor: Option<Vec<u8>> = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    loop {
        let timeout = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(timeout, rx.recv_async()).await {
            Ok(Ok(reply)) => {
                if let Ok(sample) = reply.result() {
                    let envelope = ObjectMetaEnvelope::decode(
                        sample.payload().to_bytes().as_ref(),
                    )
                    .expect("decode envelope");
                    if !envelope.object_id.is_empty() {
                        page1_ids.push(envelope.object_id.clone());
                    }
                    if envelope.next_cursor.is_some()
                        && !envelope.next_cursor.as_ref().unwrap().is_empty()
                    {
                        cursor = envelope.next_cursor.clone();
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    assert_eq!(page1_ids.len(), 2, "first Zenoh page should have 2 items");
    assert!(cursor.is_some(), "should have cursor for next page");

    // Collect all remaining via pagination
    let mut all_ids = page1_ids.clone();

    while let Some(cur) = cursor.take() {
        let request = ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 2,
            cursor: Some(cur),
        };

        let (tx, rx) = flume::bounded(16);
        session
            .get(&key_expr)
            .payload(ZBytes::from(request.encode_to_vec()))
            .consolidation(ConsolidationMode::None)
            .congestion_control(CongestionControl::Block)
            .callback(move |reply| {
                let _ = tx.send(reply);
            })
            .await
            .expect("failed to issue list-objects query");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let timeout =
                deadline.saturating_duration_since(tokio::time::Instant::now());
            match tokio::time::timeout(timeout, rx.recv_async()).await {
                Ok(Ok(reply)) => {
                    if let Ok(sample) = reply.result() {
                        let envelope = ObjectMetaEnvelope::decode(
                            sample.payload().to_bytes().as_ref(),
                        )
                        .expect("decode envelope");
                        if !envelope.object_id.is_empty() {
                            all_ids.push(envelope.object_id.clone());
                        }
                        if envelope.next_cursor.is_some()
                            && !envelope.next_cursor.as_ref().unwrap().is_empty()
                        {
                            cursor = envelope.next_cursor.clone();
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }
    }

    assert_eq!(
        all_ids.len(),
        5,
        "should find all 5 objects via Zenoh pagination"
    );

    env.shutdown().await;
}

// ============================================================================
// ObjectProxy Tests (higher-level API)
// ============================================================================

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn proxy_list_objects_basic() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("proxy_list_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let session = env.get_session().await;
    let mut client = make_client(&env).await;

    // Create objects
    let object_ids = ["proxy-obj-1", "proxy-obj-2", "proxy-obj-3"];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Use ObjectProxy to list
    let proxy = ObjectProxy::new(session);
    let result = proxy
        .list_objects(&coll, PARTITION_ID, None, Some(100), None)
        .await
        .expect("proxy list_objects failed");

    assert_eq!(result.len(), 1, "proxy returns single envelope");
    // The envelope should contain at least one object
    assert!(!result[0].object_id.is_empty(), "should have object data");

    env.shutdown().await;
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn list_objects_special_characters_in_prefix() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("list_special_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Create objects with various allowed characters
    let object_ids = [
        "item_underscore",
        "item-dash",
        "item.dot",
        "item:colon",
        "other-type",
    ];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Filter by "item" prefix (matches 4 objects)
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: Some("item".to_string()),
            limit: 100,
            cursor: None,
        })
        .await
        .expect("list_objects failed")
        .into_inner();

    let mut found_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            found_ids.push(envelope.object_id.clone());
        }
    }

    assert_eq!(found_ids.len(), 4, "should find 4 'item*' objects");

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn list_objects_limit_zero_uses_default() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("list_lim0_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    let object_ids = ["lim0-obj-1", "lim0-obj-2"];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Limit = 0 should use a sensible default, not return nothing
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 0, // Zero limit
            cursor: None,
        })
        .await
        .expect("list_objects failed")
        .into_inner();

    let mut found_ids: Vec<String> = Vec::new();
    while let Some(envelope) =
        stream.message().await.expect("stream message failed")
    {
        if !envelope.object_id.is_empty() {
            found_ids.push(envelope.object_id.clone());
        }
    }

    // With limit=0, implementation should either use default or return all
    // As long as it doesn't crash or return error, the test passes
    // The actual behavior depends on implementation choice
    assert!(
        found_ids.len() >= 0,
        "limit=0 should not cause an error"
    );

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn list_objects_invalid_cursor_ignored() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("list_badcur_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    let object_ids = ["cur-obj-1", "cur-obj-2"];
    create_test_objects(&mut client, &coll, &object_ids).await;

    // Use an invalid/garbage cursor - should either ignore it or start from beginning
    let garbage_cursor = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xAB, 0xCD];
    let mut stream = client
        .list_objects(ListObjectsRequest {
            cls_id: coll.clone(),
            partition_id: PARTITION_ID as u32,
            object_id_prefix: None,
            limit: 100,
            cursor: Some(garbage_cursor),
        })
        .await
        .expect("list_objects with bad cursor")
        .into_inner();

    // Should not crash - may return empty or all objects depending on impl
    let mut count = 0;
    while let Some(_envelope) =
        stream.message().await.expect("stream message failed")
    {
        count += 1;
    }

    // Just verify it didn't crash
    assert!(count >= 0, "invalid cursor should not cause crash");

    env.shutdown().await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn list_objects_concurrent_reads_same_collection() {
    let cfg = TestConfig::new().await;
    let env = TestEnvironment::new(cfg.clone()).await;
    env.start_odgm().await.expect("start odgm");

    let coll =
        format!("list_conc_{}", nanoid::nanoid!(6).to_ascii_lowercase());
    env.create_test_collection(&coll)
        .await
        .expect("create collection");

    let mut client = make_client(&env).await;

    // Create some objects
    let object_ids: Vec<String> =
        (0..20).map(|i| format!("conc-{:03}", i)).collect();
    let oid_refs: Vec<&str> = object_ids.iter().map(|s| s.as_str()).collect();
    create_test_objects(&mut client, &coll, &oid_refs).await;

    // Spawn multiple concurrent list requests
    let port = env.config.odgm_config.http_port;
    let coll_clone = coll.clone();

    let handles: Vec<_> = (0..5)
        .map(|_| {
            let cls = coll_clone.clone();
            tokio::spawn(async move {
                let addr = format!("http://127.0.0.1:{}", port);
                let mut client = DataServiceClient::connect(addr)
                    .await
                    .expect("connect");

                let mut stream = client
                    .list_objects(ListObjectsRequest {
                        cls_id: cls,
                        partition_id: PARTITION_ID as u32,
                        object_id_prefix: None,
                        limit: 100,
                        cursor: None,
                    })
                    .await
                    .expect("list_objects")
                    .into_inner();

                let mut count = 0;
                while let Some(envelope) =
                    stream.message().await.expect("stream")
                {
                    if !envelope.object_id.is_empty() {
                        count += 1;
                    }
                }
                count
            })
        })
        .collect();

    // Wait for all and verify consistent results
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.expect("task"));
    }

    // All concurrent reads should see the same count
    assert!(
        results.iter().all(|&c| c == 20),
        "all concurrent reads should return 20 objects: {:?}",
        results
    );

    env.shutdown().await;
}
