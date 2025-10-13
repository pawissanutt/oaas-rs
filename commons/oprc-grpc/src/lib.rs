pub mod proto {
    pub mod common {
        tonic::include_proto!("oaas.common");
    }
    pub mod package {
        tonic::include_proto!("oaas.package");
    }
    pub mod deployment {
        tonic::include_proto!("oaas.deployment");
    }
    pub mod runtime {
        tonic::include_proto!("oaas.runtime");
    }
    pub mod health {
        tonic::include_proto!("oaas.health");
    }
    pub mod oprc {
        tonic::include_proto!("oprc");
    }
}

pub mod client;
pub mod server;
pub mod types;

// Re-export all proto types at the crate root for convenience
pub use proto::common::*;
pub use proto::deployment::*;
pub use proto::health::*;
pub use proto::oprc::*;
pub use proto::package::*;
pub use proto::runtime::*;

// Re-export the unified descriptor for reflection users
#[allow(dead_code)]
pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("oaas_descriptor");

// Optional compatibility utilities equivalent to oprc-pb
#[cfg(feature = "util")]
mod util_compat {
    use super::*;
    impl From<&ObjectInvocationRequest> for ObjMeta {
        fn from(value: &ObjectInvocationRequest) -> Self {
            Self {
                partition_id: value.partition_id,
                object_id: value.object_id,
                cls_id: value.cls_id.clone(),
            }
        }
    }

    impl ObjectEvent {
        pub fn merge(&mut self, other: &ObjectEvent) {
            for (key, other_value) in &other.func_trigger {
                if let Some(value) = self.func_trigger.get_mut(key) {
                    for target in &other_value.on_complete {
                        if !value.on_complete.contains(target) {
                            value.on_complete.push(target.clone());
                        }
                    }
                    for target in &other_value.on_error {
                        if !value.on_error.contains(target) {
                            value.on_error.push(target.clone());
                        }
                    }
                } else {
                    self.func_trigger.insert(key.clone(), other_value.clone());
                }
            }
            for (key, other_value) in &other.data_trigger {
                if let Some(value) = self.data_trigger.get_mut(key) {
                    for target in &other_value.on_create {
                        if !value.on_create.contains(target) {
                            value.on_create.push(target.clone());
                        }
                    }
                    for target in &other_value.on_update {
                        if !value.on_update.contains(target) {
                            value.on_update.push(target.clone());
                        }
                    }
                    for target in &other_value.on_delete {
                        if !value.on_delete.contains(target) {
                            value.on_delete.push(target.clone());
                        }
                    }
                } else {
                    self.data_trigger.insert(*key, other_value.clone());
                }
            }
        }
    }

    use std::hash::{Hash, Hasher};
    impl Hash for ObjectEvent {
        fn hash<H: Hasher>(&self, state: &mut H) {
            for (key, value) in &self.func_trigger {
                key.hash(state);
                for target in &value.on_complete {
                    target.hash(state);
                }
                for target in &value.on_error {
                    target.hash(state);
                }
            }
            for (key, value) in &self.data_trigger {
                key.hash(state);
                for target in &value.on_create {
                    target.hash(state);
                }
                for target in &value.on_update {
                    target.hash(state);
                }
                for target in &value.on_delete {
                    target.hash(state);
                }
            }
        }
    }

    impl Hash for TriggerTarget {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.cls_id.hash(state);
            self.partition_id.hash(state);
            if let Some(obj_id) = self.object_id {
                obj_id.hash(state);
            }
            self.fn_id.hash(state);
            for (k, v) in &self.req_options {
                k.hash(state);
                v.hash(state);
            }
        }
    }

    #[cfg(feature = "util")]
    impl ObjData {
        pub fn pretty_print(&self) {
            println!("{{");
            if let Some(metadata) = &self.metadata {
                println!(
                    "  meta: {{cls_id:\"{}\", partition_id:\"{}\", object_id:\"{}\"}},",
                    metadata.cls_id, metadata.partition_id, metadata.object_id
                );
            } else {
                println!("\tmeta: NONE");
            }
            for (k, v) in self.entries.iter() {
                let s = String::from_utf8_lossy(&v.data);
                println!("  {}: {}", k, s);
            }
            println!("}}");
        }

        #[cfg(all(feature = "util", not(feature = "bytes")))]
        pub fn get_owned_entry(&self, key: u32) -> Option<Vec<u8>> {
            self.entries.get(&key).map(|v| v.data.to_owned())
        }

        #[cfg(all(feature = "util", feature = "bytes"))]
        pub fn get_owned_entry(&self, key: u32) -> Option<bytes::Bytes> {
            self.entries.get(&key).map(|v| v.data.clone())
        }
    }
}
