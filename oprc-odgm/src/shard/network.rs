use std::sync::Arc;

use flare_dht::shard::KvShard;
use oprc_pb::ObjData;
use prost::Message;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use zenoh::{bytes::ZBytes, sample::SampleKind};

use super::ObjectEntry;

type ObjectShard = Arc<dyn KvShard<Key = u64, Entry = ObjectEntry>>;

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
        let shard = self.shard.clone();
        let session = self.z_session.clone();
        let key = format!("{}/*", self.prefix);
        let token = self.token.clone();
        tokio::spawn(async move {
            let sub = session
                .declare_subscriber(key)
                .await
                .expect("Failed to declare subscriber");

            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    res = sub.recv_async() => {
                        if let Ok(sample) = res {
                            Self::handle_sample_set(&shard, sample).await
                        } else {
                            return;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    async fn start_set_queryable(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let shard = self.shard.clone();
        let session = self.z_session.clone();
        let key = format!("{}/*/set", self.prefix);
        let token = self.token.clone();
        tokio::spawn(async move {
            let queryable = session
                .declare_queryable(key)
                .await
                .expect("Failed to declare set queryable");
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    res = queryable.recv_async() => {
                        if let Ok(query) = res {
                            Self::handle_query_get(&shard, query).await
                        } else {
                            return;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    async fn start_get_queryable(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let shard = self.shard.clone();
        let session = self.z_session.clone();
        let key = format!("{}/*", self.prefix);
        let token = self.token.clone();
        tokio::spawn(async move {
            let queryable = session
                .declare_queryable(key)
                .await
                .expect("Failed to declare get queryable");
            loop {
                tokio::select! {
                    _ = token.cancelled() => {
                        break;
                    }
                    res = queryable.recv_async() => {
                        if let Ok(query) = res {
                            Self::handle_query_get(&shard, query).await
                        } else {
                            return;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    async fn handle_sample_set(
        shard: &ObjectShard,
        sample: zenoh::sample::Sample,
    ) {
        if let Some(object_id_str) = sample.key_expr().split('/').last() {
            let oid = object_id_str.parse::<u64>().unwrap();
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
        if let Some(object_id_str) = query.key_expr().split('/').last() {
            let oid = object_id_str.parse::<u64>().unwrap();
            match shard.get(&oid).await {
                Ok(Some(obj)) => {
                    let obj_data: ObjData = obj.into();
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
                    if let Err(e) = query.reply_del(query.key_expr()).await {
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

    async fn handle_query_set(shard: &ObjectShard, query: zenoh::query::Query) {
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
        if let Some(object_id_str) = query.key_expr().split('/').last() {
            let oid = object_id_str.parse::<u64>().unwrap();
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
