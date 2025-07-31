use oprc_offload::proxy::ObjectProxy;
use oprc_pb::{ObjData, ObjMeta};
use tokio::sync::watch::{Receiver, Sender};

use crate::error::OdgmError;

use super::{ObjectEntry, ShardMetadata, ShardState};

struct ProxyShardState {
    shard_metadata: ShardMetadata,
    proxy: ObjectProxy,
    readiness_sender: Sender<bool>,
    readiness_receiver: Receiver<bool>,
}

impl ProxyShardState {
    fn _new(shard_metadata: ShardMetadata, z_session: zenoh::Session) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            shard_metadata,
            proxy: ObjectProxy::new(z_session),
            readiness_sender: tx,
            readiness_receiver: rx,
        }
    }
}

#[async_trait::async_trait]
impl ShardState for ProxyShardState {
    type Key = u64;
    type Entry = ObjectEntry;
    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn initialize(&self) -> Result<(), OdgmError> {
        let _ = self.readiness_sender.send(false);
        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, OdgmError> {
        self.proxy
            .get_obj(&ObjMeta {
                cls_id: self.shard_metadata.collection.clone(),
                partition_id: self.shard_metadata.partition_id as u32,
                object_id: *key,
            })
            .await
            .map_err(|e| OdgmError::from(e))
            .map(|option_obj| option_obj.map(|o| o.into()))
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), OdgmError> {
        let mut entry: ObjData = value.into();
        entry.metadata = Some(ObjMeta {
            cls_id: self.shard_metadata.collection.clone(),
            partition_id: self.shard_metadata.partition_id as u32,
            object_id: key,
        });
        self.proxy
            .set_obj(entry)
            .await
            .map_err(|e| OdgmError::from(e))
            .map(|_| ())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError> {
        self.proxy
            .del_obj(ObjMeta {
                cls_id: self.shard_metadata.collection.clone(),
                partition_id: self.shard_metadata.partition_id as u32,
                object_id: *key,
            })
            .await
            .map_err(|e| OdgmError::from(e))
            .map(|_| ())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }
    async fn count(&self) -> Result<u64, OdgmError> {
        todo!()
    }
}
