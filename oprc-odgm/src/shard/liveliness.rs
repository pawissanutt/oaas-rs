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
    // pub async fn start(
    //     &self,
    //     z_session: zenoh::Session,
    //     token: CancellationToken,
    // ) {
    //     let liveliness_selector = format!("");
    //     let map = self.shard_liveliness.clone();
    //     tokio::spawn(async move {
    //         let liveliness_sub = match z_session
    //             .liveliness()
    //             .declare_subscriber(liveliness_selector)
    //             .await
    //         {
    //             Ok(sub) => sub,
    //             Err(e) => {
    //                 tracing::error!(
    //                     "Failed to declare liveliness subscriber: {}",
    //                     e
    //                 );
    //                 return;
    //             }
    //         };

    //         loop {
    //             tokio::select! {
    //             _ = token.cancelled() => {
    //                 break;
    //             }
    //             next = liveliness_sub.recv_async() => {
    //                 match next {
    //                     Ok(sample) => {
    //                         Self::handle_sample(map.clone(), sample).await;
    //                     }
    //                     Err(e) => {
    //                         tracing::error!("Failed to get liveliness: {}", e);
    //                     }
    //                 }
    //             }
    //             }
    //         }
    //     });
    // }

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

    pub async fn handle_sample(
        &self,
        sample: zenoh::sample::Sample,
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
