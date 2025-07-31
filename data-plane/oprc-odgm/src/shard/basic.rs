use std::{
    collections::BTreeMap,
    hash::{BuildHasherDefault, Hash},
    time::UNIX_EPOCH,
};

use automerge::AutoCommit;
use nohash_hasher::NoHashHasher;
use oprc_pb::{ObjData, ObjectResponse, ValData, ValType};

use scc::HashMap;
use tokio::sync::watch::{Receiver, Sender};

use crate::error::OdgmError;

use super::{ShardError, ShardMetadata, ShardState};

#[derive(Clone)]
pub struct BasicObjectShard {
    shard_metadata: ShardMetadata,
    map: HashMap<u64, ObjectEntry, BuildHasherDefault<NoHashHasher<u64>>>,
    _readiness_sender: Sender<bool>,
    readiness_receiver: Receiver<bool>,
}

impl BasicObjectShard {
    pub fn new(shard_metadata: ShardMetadata) -> Self {
        let (readiness_sender, readiness_receiver) =
            tokio::sync::watch::channel(true);
        Self {
            shard_metadata,
            map: HashMap::default(),
            _readiness_sender: readiness_sender,
            readiness_receiver,
        }
    }
}

#[async_trait::async_trait]
impl ShardState for BasicObjectShard {
    type Key = u64;
    type Entry = ObjectEntry;

    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn initialize(&self) -> Result<(), OdgmError> {
        self._readiness_sender.send(true).unwrap();
        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, OdgmError> {
        let out = self.map.get_async(key).await;
        let out = out.map(|r| r.clone());
        Ok(out)
    }

    // async fn modify<F, O>(
    //     &self,
    //     key: &Self::Key,
    //     processor: F,
    // ) -> Result<O, OdgmError>
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
    ) -> Result<(), OdgmError> {
        self.map.upsert_async(key, value).await;
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError> {
        self.map.remove_async(key).await;
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }
    async fn count(&self) -> Result<u64, OdgmError> {
        Ok(self.map.len() as u64)
    }
}

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Debug,
    Default,
    Clone,
    Hash,
    PartialEq,
    PartialOrd,
    Eq,
)]
pub struct ObjectVal {
    pub data: Vec<u8>,
    pub r#type: ValType,
}

impl From<&ValData> for ObjectVal {
    fn from(value: &ValData) -> Self {
        ObjectVal {
            data: value.data.clone(),
            r#type: ValType::try_from(value.r#type).unwrap_or(ValType::Byte),
        }
    }
}

impl From<ValData> for ObjectVal {
    fn from(value: ValData) -> Self {
        ObjectVal {
            data: value.data,
            r#type: ValType::try_from(value.r#type).unwrap_or(ValType::Byte),
        }
    }
}

impl ObjectVal {
    pub fn into_val(&self) -> ValData {
        ValData {
            data: self.data.clone(),
            r#type: self.r#type as i32,
        }
    }
}

#[derive(
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    PartialEq,
    // PartialOrd,
    Clone,
    Hash,
)]
pub struct ObjectEntry {
    pub last_updated: u64,
    pub value: BTreeMap<u32, ObjectVal>,
    pub event: Option<oprc_pb::ObjectEvent>,
}

impl Into<ObjData> for ObjectEntry {
    fn into(self) -> ObjData {
        ObjData {
            entries: self
                .value
                .iter()
                .map(|(i, v)| (*i, v.into_val()))
                .collect(),
            event: self.event,
            ..Default::default()
        }
    }
}

impl From<ObjData> for ObjectEntry {
    #[inline]
    fn from(value: ObjData) -> Self {
        let now = std::time::SystemTime::now();
        let ts = now
            .duration_since(UNIX_EPOCH)
            .expect("Fail to get timestamp")
            .as_millis() as u64;
        Self {
            value: value
                .entries
                .into_iter()
                .map(|(i, v)| (i, ObjectVal::from(v)))
                .collect(),
            last_updated: ts,
            event: value.event,
        }
    }
}

impl From<&ObjData> for ObjectEntry {
    #[inline]
    fn from(value: &ObjData) -> Self {
        let now = std::time::SystemTime::now();
        let ts = now
            .duration_since(UNIX_EPOCH)
            .expect("Fail to get timestamp")
            .as_millis() as u64;
        Self {
            value: value
                .entries
                .iter()
                .map(|(i, v)| (*i, ObjectVal::from(v)))
                .collect(),
            last_updated: ts,
            event: value.event.clone(),
        }
    }
}

impl ObjectEntry {
    pub fn new() -> Self {
        let now = std::time::SystemTime::now();
        let ts = now
            .duration_since(UNIX_EPOCH)
            .expect("Fail to get timestamp")
            .as_micros() as u64;
        Self {
            value: BTreeMap::new(),
            last_updated: ts,
            event: None,
        }
    }

    pub fn merge(&mut self, other: Self) -> Result<(), ShardError> {
        for (i, v2_val) in other.value.into_iter() {
            if let Some(v1_val) = self.value.get_mut(&i) {
                merge_data_owned(
                    v1_val,
                    v2_val,
                    self.last_updated < other.last_updated,
                )?;
            } else {
                if self.last_updated < other.last_updated {
                    self.value.insert(i, v2_val);
                }
            }
        }
        if self.last_updated < other.last_updated {
            self.last_updated = other.last_updated;
        }
        Ok(())
    }

    pub fn merge_cloned(&mut self, other: &Self) -> Result<(), ShardError> {
        for (i, v2_val) in other.value.iter() {
            if let Some(v1_val) = self.value.get_mut(&i) {
                merge_data(
                    v1_val,
                    v2_val,
                    self.last_updated < other.last_updated,
                )?;
            } else {
                if self.last_updated < other.last_updated {
                    self.value.insert(*i, v2_val.clone());
                }
            }
        }
        if self.last_updated < other.last_updated {
            self.last_updated = other.last_updated;
        }
        if let Some(event) = &mut self.event {
            if let Some(other_event) = &other.event {
                event.merge(other_event);
            }
        } else if other.event.is_some() {
            self.event = other.event.clone();
        }
        Ok(())
    }

    #[inline]
    pub fn to_resp(&self) -> ObjectResponse {
        ObjectResponse {
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

    pub fn random(keys: usize) -> Self {
        let mut value = BTreeMap::new();
        for i in 0..keys {
            value.insert(
                i as u32,
                ObjectVal {
                    data: rand::random::<[u8; 8]>().to_vec(),
                    r#type: ValType::Byte,
                },
            );
        }
        let now = std::time::SystemTime::now();
        let ts = now
            .duration_since(UNIX_EPOCH)
            .expect("Fail to get timestamp")
            .as_millis() as u64;
        Self {
            value,
            last_updated: ts,
            event: None,
        }
    }
}

#[allow(dead_code)]
pub fn merge_data(
    v1: &mut ObjectVal,
    v2: &ObjectVal,
    v2_older: bool,
) -> Result<(), ShardError> {
    match v1.r#type {
        ValType::Byte => {
            if v2_older {
                v1.data = v2.data.clone();
                v1.r#type = v2.r#type;
            }
        }
        ValType::CrdtMap => match v2.r#type {
            ValType::Byte => {
                if v2_older {
                    v1.data = v2.data.clone();
                    v1.r#type = v2.r#type;
                }
            }
            ValType::CrdtMap => {
                let mut v1_doc = AutoCommit::load(&v1.data[..])?;
                let mut v2_doc = AutoCommit::load(&&v2.data[..])?;
                v1_doc.merge(&mut v2_doc)?;
                v1.data = v1_doc.save();
            }
        },
    }
    Ok(())
}

pub(crate) fn merge_data_owned(
    v1: &mut ObjectVal,
    v2: ObjectVal,
    v2_older: bool,
) -> Result<(), ShardError> {
    match v1.r#type {
        ValType::Byte => {
            if v2_older {
                v1.data = v2.data;
                v1.r#type = v2.r#type;
            }
        }
        ValType::CrdtMap => match v2.r#type {
            ValType::Byte => {
                if v2_older {
                    v1.data = v2.data;
                    v1.r#type = v2.r#type;
                }
            }
            ValType::CrdtMap => {
                let mut v1_doc = AutoCommit::load(&v1.data[..])?;
                let mut v2_doc = AutoCommit::load(&&v2.data[..])?;
                v1_doc.merge(&mut v2_doc)?;
                v1.data = v1_doc.save();
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::{borrow::Cow, error::Error};

    use automerge::{transaction::Transactable, AutoCommit, ObjType, ReadDoc};

    use super::ObjectEntry;

    #[test]
    fn test_crdt() -> Result<(), Box<dyn Error>> {
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

    #[test]
    fn test_converge() {
        let mut o_1 = ObjectEntry::random(10);
        let mut o_2 = ObjectEntry::random(10);
        o_2.last_updated = o_1.last_updated + 1;
        println!("o_1: {:?}", o_1);
        println!("o_2: {:?}", o_2);
        let tmp = o_2.clone();
        o_1.merge(tmp).unwrap();
        assert_eq!(o_1, o_2);
    }
}
