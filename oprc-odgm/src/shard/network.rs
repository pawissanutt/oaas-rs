use std::sync::Arc;

use flume::Receiver;
use oprc_pb::{EmptyResponse, ObjData};
use oprc_zenoh::util::{declare_managed_subscriber, Handler};
use prost::Message;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use zenoh::{
    bytes::ZBytes,
    pubsub::Subscriber,
    query::{Query, Queryable},
    sample::{Sample, SampleKind},
};

use super::{ObjectEntry, ShardMetadata, ShardState};

type ObjectShardState = Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>;

// #[derive(Clone)]
pub struct ShardNetwork {
    z_session: zenoh::Session,
    shard: ObjectShardState,
    token: CancellationToken,
    prefix: String,
    set_subscriber: Option<Subscriber<Receiver<Sample>>>,
    set_queryable: Option<Queryable<Receiver<Query>>>,
    get_queryable: Option<Queryable<Receiver<Query>>>,
}

impl ShardNetwork {
    pub fn new(
        z_session: zenoh::Session,
        shard: ObjectShardState,
        prefix: String,
    ) -> Self {
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            shard,
            token,
            prefix,
            set_subscriber: None,
            set_queryable: None,
            get_queryable: None,
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.token = CancellationToken::new();
        self.start_set_subscriber().await?;
        self.start_set_queryable().await?;
        self.start_get_queryable().await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(id=%self.shard.meta().id))]
    pub async fn stop(&mut self) {
        if let Some(sub) = self.set_subscriber.take() {
            if let Err(e) = sub.undeclare().await {
                tracing::warn!("Failed to undeclare subscriber: {}", e);
            };
        }
        if let Some(q) = self.set_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
        if let Some(q) = self.get_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
    }

    pub fn is_running(&self) -> bool {
        !self.token.is_cancelled()
    }

    async fn start_set_subscriber(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*", self.prefix);
        let handler = SetterHandler {
            shard: self.shard.clone(),
        };
        let s = declare_managed_subscriber(
            &self.z_session,
            key,
            handler,
            16,
            65536,
        )
        .await?;
        self.set_subscriber = Some(s);
        Ok(())
    }

    async fn start_set_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*/set", self.prefix);
        let handler = SetterHandler {
            shard: self.shard.clone(),
        };
        let q = oprc_zenoh::util::declare_managed_queryable(
            &self.z_session,
            key,
            handler,
            16,
            65536,
        )
        .await?;
        self.set_queryable = Some(q);
        Ok(())
    }

    async fn start_get_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*", self.prefix);
        let handler = GetterHandler {
            shard: self.shard.clone(),
        };
        let q = oprc_zenoh::util::declare_managed_queryable(
            &self.z_session,
            key,
            handler,
            16,
            65536,
        )
        .await?;
        self.get_queryable = Some(q);

        Ok(())
    }
}

#[inline]
async fn parse_oid_from_query(
    shard_id: u64,
    query: &zenoh::query::Query,
) -> Option<u64> {
    if let Some(object_id_str) = query.key_expr().split('/').skip(4).next() {
        let oid = match object_id_str.parse::<u64>() {
            Ok(oid) => oid,
            Err(e) => {
                warn!("(shard={}) Failed to parse object ID: {}", shard_id, e);
                if let Err(reply_err) =
                    query.reply_err(ZBytes::from("invalid object ID")).await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard: {}",
                        shard_id, reply_err
                    );
                }
                return None;
            }
        };
        Some(oid)
    } else {
        tracing::debug!(
            "(shard={}) Failed to get object ID from keys: {}",
            shard_id,
            query.key_expr()
        );
        None
    }
}

#[derive(Clone)]
struct SetterHandler {
    shard: ObjectShardState,
}

#[async_trait::async_trait]
impl Handler<Query> for SetterHandler {
    async fn handle(&self, query: Query) {
        let id = self.shard.meta().id;
        tracing::debug!("Shard Network '{}': Handling set query", id);
        if query.payload().is_none() {
            if let Err(e) =
                query.reply_err(ZBytes::from("payload is required")).await
            {
                warn!("(shard={}) Failed to reply error for shard: {}", id, e);
            }
            return;
        }
        let parsed_id = parse_oid_from_query(id, &query).await;
        if let Some(oid) = parsed_id {
            match ObjData::decode(query.payload().unwrap().to_bytes().as_ref())
            {
                Ok(obj_data) => {
                    let obj_entry = ObjectEntry::from(obj_data);
                    if let Err(e) = self.shard.set(oid, obj_entry).await {
                        warn!("Failed to set object for shard {}: {}", id, e);
                    } else {
                        let payload = EmptyResponse::default().encode_to_vec();
                        let payload = ZBytes::from(payload);
                        if let Err(e) =
                            query.reply(query.key_expr(), payload).await
                        {
                            warn!(
                                "(shard={}) Failed to reply for shard: {}",
                                id, e
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("(shard={}) Failed to decode object data: {}", id, e)
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl Handler<Sample> for SetterHandler {
    async fn handle(&self, sample: Sample) {
        let id = self.shard.meta().id;
        if let Some(oid) = parse_oid_from_sample(id, &sample) {
            match sample.kind() {
                SampleKind::Put => {
                    match ObjData::decode(sample.payload().to_bytes().as_ref())
                    {
                        Ok(obj_data) => {
                            let obj_entry = ObjectEntry::from(obj_data);
                            if let Err(e) = self.shard.set(oid, obj_entry).await
                            {
                                warn!(
                                    "(shard={}) Failed to set object: {}",
                                    id, e
                                );
                            }
                        }
                        Err(e) => warn!(
                            "(shard={}) Failed to decode object data: {}",
                            id, e
                        ),
                    }
                }
                SampleKind::Delete => {
                    if let Err(e) = self.shard.as_ref().delete(&oid).await {
                        warn!("(shard={}) Failed to set object: {}", id, e);
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct GetterHandler {
    shard: ObjectShardState,
}

#[async_trait::async_trait]
impl Handler<Query> for GetterHandler {
    async fn handle(&self, query: Query) {
        let id = self.shard.meta().id;
        if let Some(oid) = parse_oid_from_query(id, &query).await {
            match self.shard.get(&oid).await {
                Ok(Some(obj)) => {
                    let mut obj_data: ObjData = obj.into();
                    obj_data.metadata =
                        Some(enrich_obj(oid, self.shard.meta()));
                    let obj_data_bytes = obj_data.encode_to_vec();
                    if let Err(e) =
                        query.reply(query.key_expr(), obj_data_bytes).await
                    {
                        warn!(
                            "(shard={}) Failed to reply with object data: {}",
                            id, e
                        );
                    }
                }
                Ok(None) => {
                    let payload = EmptyResponse::default().encode_to_vec();
                    if let Err(e) = query.reply(query.key_expr(), payload).await
                    {
                        warn!("(shard={}) Failed to reply delete: {}", id, e);
                    }
                }
                Err(e) => {
                    warn!("(shard={}) Failed to get object: {}", id, e)
                }
            };
        }
    }
}

#[inline]
fn parse_oid_from_sample(
    shard_id: u64,
    query: &zenoh::sample::Sample,
) -> Option<u64> {
    if let Some(object_id_str) = query.key_expr().split('/').skip(4).next() {
        let oid = match object_id_str.parse::<u64>() {
            Ok(id) => id,
            Err(e) => {
                warn!("(shard={}) Failed to parse object ID: {}", shard_id, e);
                return None;
            }
        };
        Some(oid)
    } else {
        tracing::debug!(
            "(shard={}) Failed to get object ID from keys: {}",
            shard_id,
            query.key_expr()
        );
        None
    }
}

#[inline]
fn enrich_obj(id: u64, meta: &ShardMetadata) -> oprc_pb::ObjMeta {
    oprc_pb::ObjMeta {
        partition_id: meta.partition_id as u32,
        object_id: id,
        cls_id: meta.collection.clone(),
        ..Default::default()
    }
}
