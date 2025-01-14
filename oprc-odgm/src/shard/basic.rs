use std::{cmp::Ordering, collections::BTreeMap, hash::Hash};

use automerge::AutoCommit;
use flare_dht::{error::FlareError, shard::ShardMetadata};
use oprc_pb::{val_data::Data, ObjData, ObjectReponse, ValData};

use prost::bytes::Bytes;
use scc::HashMap;

use super::{ShardState, ShardError};

#[derive(Clone)]
pub struct ObjectShard {
    pub(crate) shard_metadata: ShardMetadata,
    pub(crate) map: HashMap<u64, ObjectEntry>,
}

#[async_trait::async_trait]
impl ShardState for ObjectShard {
    type Key = u64;
    type Entry = ObjectEntry;

    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, FlareError> {
        let out = self.map.get_async(key).await;
        let out = out.map(|r| r.clone());
        Ok(out)
    }

    // async fn modify<F, O>(
    //     &self,
    //     key: &Self::Key,
    //     processor: F,
    // ) -> Result<O, FlareError>
    // where
    //     F: FnOnce(&mut Self::Entry) -> O + Send,
    // {
    //     let out = match self.map.entry_async(key.clone()).await {
    //         Occupied(mut occupied_entry) => {
    //             let entry = occupied_entry.get_mut();
    //             let o = processor(entry);
    //             o
    //         }
    //         Vacant(vacant_entry) => {
    //             let mut entry = Self::Entry::default();
    //             let o = processor(&mut entry);
    //             vacant_entry.insert_entry(entry);
    //             o
    //         }
    //     };
    //     Ok(out)
    // }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), FlareError> {
        self.map.upsert_async(key, value).await;
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError> {
        self.map.remove_async(key).await;
        Ok(())
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Hash)]
pub enum ObjectVal {
    Byte(Bytes),
    CRDT(Bytes),
    None,
}

impl From<&ValData> for ObjectVal {
    fn from(value: &ValData) -> Self {
        match &value.data {
            Some(val_data) => match val_data {
                Data::Byte(bytes) => ObjectVal::Byte(bytes.to_owned()),
                Data::CrdtMap(bytes) => ObjectVal::CRDT(bytes.to_owned()),
            },
            None => ObjectVal::None,
        }
    }
}

impl From<ValData> for ObjectVal {
    fn from(value: ValData) -> Self {
        match value.data {
            Some(val_data) => match val_data {
                Data::Byte(bytes) => ObjectVal::Byte(bytes),
                Data::CrdtMap(bytes) => ObjectVal::CRDT(bytes),
            },
            None => ObjectVal::None,
        }
    }
}

impl ObjectVal {
    pub fn into_val(&self) -> ValData {
        match &self {
            ObjectVal::Byte(bytes) => ValData {
                data: Some(Data::Byte(bytes.to_owned())),
            },
            ObjectVal::CRDT(bytes) => ValData {
                data: Some(Data::CrdtMap(bytes.to_owned())),
            },
            ObjectVal::None => ValData { data: None },
        }
    }
}

impl PartialEq for ObjectVal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ObjectVal::Byte(a), ObjectVal::Byte(b)) => {
                a.as_ref() == b.as_ref()
            }
            (ObjectVal::CRDT(a), ObjectVal::CRDT(b)) => {
                a.as_ref() == b.as_ref()
            }
            _ => false,
        }
    }
}

impl PartialOrd for ObjectVal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (ObjectVal::Byte(a), ObjectVal::Byte(b)) => {
                a.as_ref().partial_cmp(b.as_ref())
            }
            (ObjectVal::CRDT(a), ObjectVal::CRDT(b)) => {
                a.as_ref().partial_cmp(b.as_ref())
            }
            // Handle mixed comparisons if needed
            (ObjectVal::Byte(a), ObjectVal::CRDT(b)) => {
                a.as_ref().partial_cmp(b.as_ref())
            }
            (ObjectVal::CRDT(a), ObjectVal::Byte(b)) => {
                a.as_ref().partial_cmp(b.as_ref())
            }
            (ObjectVal::None, _) => Some(Ordering::Greater),
            (_, ObjectVal::None) => Some(Ordering::Less),
        }
    }
}

#[derive(
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    PartialEq,
    PartialOrd,
    Clone,
    Hash,
)]
pub struct ObjectEntry {
    // pub rc: u16,
    pub value: BTreeMap<u32, ObjectVal>,
}

impl Into<ObjData> for ObjectEntry {
    fn into(self) -> ObjData {
        ObjData {
            entries: self
                .value
                .iter()
                .map(|(i, v)| (*i, v.into_val()))
                .collect(),
            ..Default::default()
        }
    }
}

impl From<ObjData> for ObjectEntry {
    #[inline]
    fn from(value: ObjData) -> Self {
        Self {
            value: value
                .entries
                .into_iter()
                .map(|(i, v)| (i, ObjectVal::from(v)))
                .collect(),
        }
    }
}

impl From<&ObjData> for ObjectEntry {
    #[inline]
    fn from(value: &ObjData) -> Self {
        Self {
            value: value
                .entries
                .iter()
                .map(|(i, v)| (*i, ObjectVal::from(v)))
                .collect(),
        }
    }
}

impl ObjectEntry {
    pub fn merge(&mut self, other: &Self) -> Result<(), ShardError> {
        for (i, v2_val) in other.value.iter() {
            if let Some(v1_val) = self.value.get_mut(i) {
                merge_data(v1_val, v2_val)?;
            } else {
                self.value.insert(*i, v2_val.clone());
            }
        }
        Ok(())
    }

    #[inline]
    pub fn to_resp(&self) -> ObjectReponse {
        ObjectReponse {
            obj: Some(self.to_data()),
        }
    }

    #[inline]
    pub fn to_data(&self) -> ObjData {
        ObjData {
            entries: self
                .value
                .iter()
                .map(|(i, v)| (*i, v.into_val()))
                .collect(),
            ..Default::default()
        }
    }
}

pub(crate) fn merge_data(
    v1: &mut ObjectVal,
    v2: &ObjectVal,
) -> Result<ObjectVal, ShardError> {
    match v1 {
        ObjectVal::Byte(_) => Ok(v2.to_owned()),
        ObjectVal::CRDT(v1_data) => {
            if let ObjectVal::CRDT(v2_data) = &v2 {
                let mut v1_doc = AutoCommit::load(v1_data.as_ref())?;
                let mut v2_doc = AutoCommit::load(v2_data.as_ref())?;
                v1_doc.merge(&mut v2_doc)?;
                let b = Bytes::from(v1_doc.save());
                Ok(ObjectVal::CRDT(b))
            } else {
                Ok(v1.to_owned())
            }
        }
        ObjectVal::None => Ok(v2.to_owned()),
    }
}

#[cfg(test)]
mod test {
    use std::{borrow::Cow, error::Error};

    use automerge::{transaction::Transactable, AutoCommit, ObjType, ReadDoc};

    #[test]
    fn test() -> Result<(), Box<dyn Error>> {
        let mut doc1 = AutoCommit::new();
        let contacts =
            doc1.put_object(automerge::ROOT, "contacts", ObjType::List)?;

        // Now we can insert objects into the list
        let alice = doc1.insert_object(&contacts, 0, ObjType::Map)?;

        // Finally we can set keys in the "alice" map
        doc1.put(&alice, "name", "Alice")?;
        doc1.put(&alice, "email", "alice@example.com")?;

        // Create another contact
        let bob = doc1.insert_object(&contacts, 1, ObjType::Map)?;
        doc1.put(&bob, "name", "Bob")?;
        doc1.put(&bob, "email", "bob@example.com")?;

        // Now we save the address book, we can put this in a file
        let data = doc1.save();
        let mut doc2 = AutoCommit::load(&data)?;
        let contacts = match doc2.get(automerge::ROOT, "contacts")? {
            Some((automerge::Value::Object(ObjType::List), contacts)) => {
                contacts
            }
            _ => panic!("contacts should be a list"),
        };
        let bob = match doc2.get(&contacts, 1)? {
            Some((automerge::Value::Object(ObjType::Map), bob)) => bob,
            _ => panic!("bob should be a map"),
        };
        doc2.put(&bob, "name", "Robert")?;

        // Finally, we can merge the changes from the two devices
        doc1.merge(&mut doc2)?;
        let bobsname: Option<automerge::Value> =
            doc1.get(&bob, "name")?.map(|(v, _)| v);

        assert_eq!(
            bobsname,
            Some(automerge::Value::Scalar(Cow::Owned("Robert".into())))
        );
        Ok(())
    }
}
