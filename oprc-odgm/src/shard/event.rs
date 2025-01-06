use std::error::Error;

use flare_zrpc::MsgSerde;
use oprc_zenoh::ServiceIdentifier;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::error;

use super::ObjectEntry;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) enum ObjectChangedEvent {
    Update(ObjectEntry),
    Delete(u64),
}

type EventSerde = flare_zrpc::bincode::BincodeMsgSerde<ObjectChangedEvent>;

#[derive(Debug)]
pub struct ZenohEventPublisher {
    id: oprc_zenoh::ServiceIdentifier,
    z_session: zenoh::Session,
}

impl ZenohEventPublisher {
    pub fn new(
        id: oprc_zenoh::ServiceIdentifier,
        z_session: zenoh::Session,
    ) -> Self {
        ZenohEventPublisher { id, z_session }
    }

    pub fn pipe(&self, mut receiver: UnboundedReceiver<ObjectChangedEvent>) {
        let z_session = self.z_session.clone();
        let id = self.id.clone();
        tokio::spawn(async move {
            loop {
                if let Some(event) = receiver.recv().await {
                    if let Err(e) =
                        Self::pub_event(&z_session, &id, event).await
                    {
                        error!(
                            "error on publishing object changed event: {:?}",
                            e
                        );
                    };
                } else {
                    break;
                }
            }
        });
    }

    async fn pub_event(
        z_session: &zenoh::Session,
        id: &ServiceIdentifier,
        event: ObjectChangedEvent,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let key = format!(
            "oprc/{}/{}/replica/{}/objects/events",
            id.class_id, id.partition_id, id.replica_id
        );
        z_session.put(key, EventSerde::to_zbyte(&event)?).await?;
        Ok(())
    }
}
