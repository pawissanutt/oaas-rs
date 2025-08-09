use std::{collections::BTreeMap, hash::Hash, time::UNIX_EPOCH};

use automerge::AutoCommit;
use oprc_pb::{ObjData, ObjectResponse, ValData, ValType};

/// Object-level errors for merge and data operations
#[derive(Debug, thiserror::Error)]
pub enum ObjectError {
    #[error("CRDT merge error: {0}")]
    CrdtError(#[from] automerge::AutomergeError),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Invalid data format: {0}")]
    InvalidDataFormat(String),
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

    pub fn merge(&mut self, other: Self) -> Result<(), ObjectError> {
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

    pub fn merge_cloned(&mut self, other: &Self) -> Result<(), ObjectError> {
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
) -> Result<(), ObjectError> {
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
) -> Result<(), ObjectError> {
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
    use oprc_pb::{ObjData, ValData};

    use crate::shard::ObjectVal;

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

    #[test]
    fn test_object_entry_bincode_serialization() {
        use std::collections::HashMap;

        // Create a simple ObjectEntry like the test does
        let mut entries = HashMap::new();
        entries.insert(
            1,
            ValData {
                data: b"test_value".to_vec(),
                r#type: 0, // VAL_TYPE_BYTE = 0
            },
        );

        let obj_data = ObjData {
            metadata: None,
            entries,
            event: None,
        };

        // Convert to ObjectEntry (this is what happens in the set operation)
        let object_entry = ObjectEntry::from(obj_data);

        // Try to serialize and deserialize with bincode
        let serialized = bincode::serde::encode_to_vec(
            &object_entry,
            bincode::config::standard(),
        );
        assert!(
            serialized.is_ok(),
            "Serialization failed: {:?}",
            serialized.err()
        );

        let serialized_bytes = serialized.unwrap();
        let deserialized = bincode::serde::decode_from_slice(
            &serialized_bytes,
            bincode::config::standard(),
        );

        assert!(
            deserialized.is_ok(),
            "Deserialization failed: {:?}",
            deserialized.err()
        );

        let (deserialized_entry, _): (ObjectEntry, usize) =
            deserialized.unwrap();
        assert_eq!(object_entry.value, deserialized_entry.value);
    }

    #[test]
    fn test_object_val_direct() {
        // Test ObjectVal directly
        let val_data = ValData {
            data: b"test_value".to_vec(),
            r#type: 0,
        };

        let object_val = ObjectVal::from(val_data);

        // Try to serialize and deserialize ObjectVal directly
        let serialized = bincode::serde::encode_to_vec(
            &object_val,
            bincode::config::standard(),
        );
        assert!(
            serialized.is_ok(),
            "ObjectVal serialization failed: {:?}",
            serialized.err()
        );

        let serialized_bytes = serialized.unwrap();
        let deserialized = bincode::serde::decode_from_slice(
            &serialized_bytes,
            bincode::config::standard(),
        );

        assert!(
            deserialized.is_ok(),
            "ObjectVal deserialization failed: {:?}",
            deserialized.err()
        );
        let (deserialized_val, _): (ObjectVal, usize) = deserialized.unwrap();
        assert_eq!(object_val, deserialized_val);
    }
}
