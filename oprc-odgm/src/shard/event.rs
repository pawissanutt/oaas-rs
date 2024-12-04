use std::error::Error;

use oprc_zenoh::rpc::MsgSerde;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::error;

use super::ObjectEntry;

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) enum ObjectChangedEvent {
    Update(ObjectEntry),
    Delete(u64),
}

struct EventSerde {}

impl MsgSerde for EventSerde {
    type Data = ObjectChangedEvent;
}
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

    pub fn pipe(self, mut receiver: UnboundedReceiver<ObjectChangedEvent>) {
        tokio::spawn(async move {
            loop {
                if let Some(event) = receiver.recv().await {
                    if let Err(e) = self.pub_event(event).await {
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

    pub async fn pub_event(
        &self,
        event: ObjectChangedEvent,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let key = format!(
            "oprc/{}/{}/replica/{}/objects/events",
            self.id.class_id, self.id.partition_id, self.id.replica_id
        );
        self.z_session
            .put(key, EventSerde::to_zbyte(event).unwrap())
            .await?;
        Ok(())
    }
}
