use envconfig::Envconfig;
use oprc_dp_storage::AnyStorage;
use oprc_grpc::ValType;
use oprc_odgm::granular_trait::EntryStore;
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::ObjectVal;
use oprc_odgm::shard::ShardOptions;
use oprc_odgm::shard::traits::ShardMetadata;
use oprc_odgm::shard::{
    ObjectUnifiedShard, ShardBuilder, ShardError,
};

type TestShard = ObjectUnifiedShard<
    AnyStorage,
    NoReplication<AnyStorage>,
    oprc_odgm::events::EventManagerImpl<AnyStorage>,
>;

fn metadata() -> ShardMetadata {
    ShardMetadata {
        id: 8601,
        collection: "mutation".into(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "memory".into(),
        options: Default::default(),
        invocations: Default::default(),
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}
fn val(d: &str) -> ObjectVal {
    ObjectVal {
        data: d.as_bytes().to_vec(),
        r#type: ValType::Byte,
    }
}

async fn make_shard() -> Result<TestShard, ShardError> {
    unsafe { std::env::set_var("ODGM_EVENT_PIPELINE_V2", "true") };
    let z_conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let pool = oprc_zenoh::pool::Pool::new(1, z_conf);
    let session = pool.get_session().await.map_err(|e| {
        ShardError::ConfigurationError(format!("Failed to get session: {}", e))
    })?;
    ShardBuilder::new()
        .metadata(metadata())
        .session(session)
        .options(ShardOptions::new(160, 256))
        .memory_storage()?
        .no_replication()
        .build()
        .await
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn test_mutation_context_create_update_counts() -> Result<(), ShardError>
{
    let shard = make_shard().await?;
    let oid = "tenant::mc";
    shard.set_entry(oid, "k1", val("v1")).await?; // create
    shard.set_entry(oid, "k1", val("v2")).await?; // update
    shard
        .batch_set_entries(
            oid,
            vec![("k2".to_string(), val("v3")), ("k3".to_string(), val("v4"))]
                .into_iter()
                .collect(),
            None,
        )
        .await?; // creates
    // Bridge is auto-disabled in V2 mode; instead assert metadata version advanced correctly: 3 mutations -> version 3
    let meta = shard.get_metadata(oid).await?.unwrap();
    assert_eq!(
        meta.object_version, 3,
        "expected object_version 3 after create, update, batch"
    );
    Ok(())
}
