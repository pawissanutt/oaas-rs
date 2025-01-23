use std::sync::Arc;

use oprc_pb::{EmptyResponse, ObjData};
use prost::Message;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use zenoh::{bytes::ZBytes, sample::SampleKind};

use super::{ObjectEntry, ShardMetadata, ShardState};

type ObjectShard = Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>;

#[derive(Clone)]
pub struct ShardNetwork {
    z_session: zenoh::Session,
    shard: ObjectShard,
    token: CancellationToken,
    prefix: String,
}

impl ShardNetwork {
    pub fn new(
        z_session: zenoh::Session,
        shard: ObjectShard,
        prefix: String,
    ) -> Self {
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            shard,
            token,
            prefix,
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

    pub fn stop(&self) {
        self.token.cancel();
    }

    pub fn is_running(&self) -> bool {
        !self.token.is_cancelled()
    }

    async fn start_set_subscriber(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*", self.prefix);
        let (tx, rx) = flume::unbounded();
        self.z_session
            .declare_subscriber(key.clone())
            .callback(move |query| tx.send(query).unwrap())
            .background()
            .await?;
        tracing::info!("shard network '{}': sub: declared", key);
        for _ in 0..16 {
            let local_token = self.token.clone();
            let local_rx = rx.clone();
            let shard = self.shard.clone();
            let ke = key.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        query_res = local_rx.recv_async() => match query_res {
                            Ok(query) =>  Self::handle_sample_set(&shard, query).await,
                            Err(err) => {
                                tracing::error!("shard network '{}': sub:error: {}", ke, err,);
                                break;
                            }
                        },
                        _ = local_token.cancelled() => {
                            break;
                        }
                    }
                }
                tracing::info!("shard network '{}': sub: cancelled", ke);
            });
        }
        Ok(())
    }

    async fn start_set_queryable(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*/set", self.prefix);
        let (tx, rx) = flume::bounded(1024);
        self.z_session
            .declare_queryable(key.clone())
            .complete(true)
            .callback(move |query| tx.send(query).unwrap())
            .background()
            .await?;
        tracing::info!("shard network '{}': set: declared", key);
        for _ in 0..16 {
            let local_token = self.token.clone();
            let local_rx = rx.clone();
            let shard = self.shard.clone();
            let ke = key.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        query_res = local_rx.recv_async() => match query_res {
                            Ok(query) =>  Self::handle_query_set(&shard, query).await,
                            Err(err) => {
                                tracing::error!("shard network '{}': set: error: {}", ke, err,);
                                break;
                            }
                        },
                        _ = local_token.cancelled() => {
                            break;
                        }
                    }
                }
                tracing::info!("shard network '{}': set: cancelled", ke);
            });
        }
        Ok(())
    }

    async fn start_get_queryable(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*", self.prefix);
        let (tx, rx) = flume::bounded(1024);
        self.z_session
            .declare_queryable(key.clone())
            .complete(true)
            .callback(move |query| tx.send(query).unwrap())
            .background()
            .await?;
        tracing::info!("shard network '{}': get: declared", key);
        for _ in 0..16 {
            let local_token = self.token.clone();
            let local_rx = rx.clone();
            let shard = self.shard.clone();
            let ke = key.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        query_res = local_rx.recv_async() => match query_res {
                            Ok(query) =>  Self::handle_query_get(&shard, query).await,
                            Err(err) => {
                                tracing::error!("shard network '{}': error: {}", ke, err,);
                                break;
                            }
                        },
                        _ = local_token.cancelled() => {
                            break;
                        }
                    }
                }
                tracing::info!("shard network '{}': get: cancelled", ke);
            });
        }
        Ok(())
    }

    async fn handle_sample_set(
        shard: &ObjectShard,
        sample: zenoh::sample::Sample,
    ) {
        if let Some(oid) = parse_oid_from_sample(shard.meta().id, &sample) {
            match sample.kind() {
                SampleKind::Put => {
                    match ObjData::decode(sample.payload().to_bytes().as_ref())
                    {
                        Ok(obj_data) => {
                            let obj_entry = ObjectEntry::from(obj_data);
                            if let Err(e) = shard.set(oid, obj_entry).await {
                                warn!(
                                    "(shard={}) Failed to set object: {}",
                                    shard.meta().id,
                                    e
                                );
                            }
                        }
                        Err(e) => warn!(
                            "(shard={}) Failed to decode object data: {}",
                            shard.meta().id,
                            e
                        ),
                    }
                }
                SampleKind::Delete => {
                    if let Err(e) = shard.as_ref().delete(&oid).await {
                        warn!(
                            "(shard={}) Failed to set object: {}",
                            shard.meta().id,
                            e
                        );
                    }
                }
            }
        }
    }

    async fn handle_query_get(shard: &ObjectShard, query: zenoh::query::Query) {
        if let Some(oid) = parse_oid_from_query(shard.meta().id, &query).await {
            match shard.get(&oid).await {
                Ok(Some(obj)) => {
                    let mut obj_data: ObjData = obj.into();
                    obj_data.metadata = Some(enrich_obj(oid, shard.meta()));
                    let obj_data_bytes = obj_data.encode_to_vec();
                    if let Err(e) =
                        query.reply(query.key_expr(), obj_data_bytes).await
                    {
                        warn!(
                            "(shard={}) Failed to reply with object data: {}",
                            shard.meta().id,
                            e
                        );
                    }
                }
                Ok(None) => {
                    let payload = EmptyResponse::default().encode_to_vec();
                    if let Err(e) = query.reply(query.key_expr(), payload).await
                    {
                        warn!(
                            "(shard={}) Failed to reply delete: {}",
                            shard.meta().id,
                            e
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "(shard={}) Failed to get object: {}",
                        shard.meta().id,
                        e
                    )
                }
            };
        }
    }

    #[tracing::instrument(skip(shard), level = "debug")]
    async fn handle_query_set(shard: &ObjectShard, query: zenoh::query::Query) {
        tracing::debug!(
            "Shard Network '{}': Handling set query",
            shard.meta().id
        );
        if query.payload().is_none() {
            if let Err(e) =
                query.reply_err(ZBytes::from("payload is required")).await
            {
                warn!(
                    "(shard={}) Failed to reply error for shard: {}",
                    shard.meta().id,
                    e
                );
            }
            return;
        }
        let parsed_id = parse_oid_from_query(shard.meta().id, &query).await;
        if let Some(oid) = parsed_id {
            match ObjData::decode(query.payload().unwrap().to_bytes().as_ref())
            {
                Ok(obj_data) => {
                    let obj_entry = ObjectEntry::from(obj_data);
                    if let Err(e) = shard.set(oid, obj_entry).await {
                        warn!(
                            "Failed to set object for shard {}: {}",
                            shard.meta().id,
                            e
                        );
                    } else {
                        let payload = EmptyResponse::default().encode_to_vec();
                        let payload = ZBytes::from(payload);
                        if let Err(e) =
                            query.reply(query.key_expr(), payload).await
                        {
                            warn!(
                                "(shard={}) Failed to reply for shard: {}",
                                shard.meta().id,
                                e
                            );
                        }
                    }
                }
                Err(e) => warn!(
                    "(shard={}) Failed to decode object data: {}",
                    shard.meta().id,
                    e
                ),
            }
        }
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
