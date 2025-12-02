pub mod proto {
    pub mod common {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oaas.common");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oaas.common.rs"));
    }
    pub mod package {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oaas.package");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oaas.package.rs"));
    }
    pub mod deployment {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oaas.deployment");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oaas.deployment.rs"));
    }
    pub mod runtime {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oaas.runtime");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oaas.runtime.rs"));
    }
    pub mod health {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oaas.health");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oaas.health.rs"));
    }
    pub mod topology {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oaas.topology");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oaas.topology.rs"));
    }
    pub mod oprc {
        #[cfg(feature = "grpc")]
        tonic::include_proto!("oprc");
        #[cfg(not(feature = "grpc"))]
        include!(concat!(env!("OUT_DIR"), "/oprc.rs"));
    }
}

#[cfg(feature = "grpc")]
pub mod client;
#[cfg(feature = "grpc")]
pub mod server;
pub mod types;

#[cfg(feature = "otel")]
pub mod tracing;

// Re-export tonic types for convenience
#[cfg(feature = "grpc")]
pub use tonic::Request;

// Re-export all proto types at the crate root for convenience
pub use proto::common::*;
pub use proto::deployment::*;
pub use proto::health::*;
pub use proto::oprc::*;
pub use proto::package::*;
pub use proto::runtime::*;
pub use proto::topology::*;

impl TriggerTarget {
    pub fn stateless(
        cls_id: impl Into<String>,
        partition_id: u32,
        fn_id: impl Into<String>,
    ) -> Self {
        Self {
            cls_id: cls_id.into(),
            partition_id,
            object_id: None,
            fn_id: fn_id.into(),
            req_options: Default::default(),
        }
    }

    pub fn for_object_str(
        cls_id: impl Into<String>,
        partition_id: u32,
        object_id_str: impl Into<String>,
        fn_id: impl Into<String>,
    ) -> Self {
        Self {
            cls_id: cls_id.into(),
            partition_id,
            object_id: Some(object_id_str.into()),
            fn_id: fn_id.into(),
            req_options: Default::default(),
        }
    }
}

#[allow(dead_code)]
#[cfg(feature = "grpc")]
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
                object_id: value.object_id.clone(),
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
                    self.data_trigger.insert(key.clone(), other_value.clone());
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

    #[allow(deprecated)]
    impl Hash for TriggerTarget {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.cls_id.hash(state);
            self.partition_id.hash(state);
            if let Some(obj_id) = &self.object_id {
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
                if let Some(oid) = &metadata.object_id {
                    println!(
                        "  meta: {{cls_id:\"{}\", partition_id:\"{}\", object_id:\"{}\"}},",
                        metadata.cls_id, metadata.partition_id, oid
                    );
                } else {
                    println!(
                        "  meta: {{cls_id:\"{}\", partition_id:\"{}\", object_id:NONE}},",
                        metadata.cls_id, metadata.partition_id
                    );
                }
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
        pub fn get_owned_entry(&self, key: &str) -> Option<Vec<u8>> {
            self.entries.get(key).map(|v| v.data.to_owned())
        }

        #[cfg(all(feature = "util", feature = "bytes"))]
        pub fn get_owned_entry(&self, key: &str) -> Option<bytes::Bytes> {
            self.entries.get(key).map(|v| v.data.clone())
        }
    }
}
