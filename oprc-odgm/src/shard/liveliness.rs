use std::{hash::BuildHasherDefault, sync::Arc};

use nohash_hasher::NoHashHasher;
use scc::HashMap;
use tokio::sync::Mutex;
use zenoh::{liveliness::LivelinessToken, sample::SampleKind};

use super::ShardMetadata;

type LivelinessMap =
    Arc<HashMap<u64, bool, BuildHasherDefault<NoHashHasher<u64>>>>;

#[derive(Clone, Default)]
pub struct MemberLivelinessState {
    pub liveliness_map: LivelinessMap,
    local_token: Arc<Mutex<Option<LivelinessToken>>>,
}

impl MemberLivelinessState {
    pub async fn declare_liveliness(
        &self,
        z_session: &zenoh::Session,
        meta: &ShardMetadata,
    ) {
        let key = format!(
            "oprc/{}/{}/liveliness/{}",
            meta.collection, meta.partition_id, meta.id
        );
        match z_session.liveliness().declare_token(key).await {
            Ok(token) => {
                self.local_token.lock().await.replace(token);
            }
            Err(err) => {
                tracing::error!(
                    "shard {}: Failed to declare liveliness token: {}",
                    meta.id,
                    err
                );
            }
        }
    }

    pub async fn update(
        &self,
        z_session: &zenoh::Session,
        meta: &ShardMetadata,
    ) {
        let key = format!(
            "oprc/{}/{}/liveliness/*",
            meta.collection, meta.partition_id
        );
        if let Ok(result) = z_session.liveliness().get(key).await {
            while let Ok(reply) = result.recv_async().await {
                match reply.result() {
                    Ok(sample) => {
                        self.handle_sample(sample).await;
                    }
                    Err(_) => continue,
                }
            }
        }
    }

    pub async fn handle_sample(
        &self,
        sample: &zenoh::sample::Sample,
    ) -> Option<u64> {
        let id = sample
            .key_expr()
            .split("/")
            .skip(4)
            .next()
            .map(|id_str| id_str.parse::<u64>());
        if let Some(Ok(id)) = id {
            match sample.kind() {
                SampleKind::Put => {
                    self.liveliness_map.upsert_async(id, true).await;
                }
                SampleKind::Delete => {
                    self.liveliness_map.upsert_async(id, false).await;
                }
            }
            return Some(id);
        }
        None
    }
}
