tonic::include_proto!("oprc");
#[allow(dead_code)]
pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("oaas_descriptor");

impl From<&ObjectInvocationRequest> for ObjMeta {
    fn from(value: &ObjectInvocationRequest) -> Self {
        Self {
            partition_id: value.partition_id,
            object_id: value.object_id,
            cls_id: value.cls_id.clone(),
        }
    }
}

mod util;

impl ObjectEvent {
    pub fn merge(&mut self, other: &ObjectEvent) {
        // Merge function triggers
        for (key, other_value) in &other.func_trigger {
            if let Some(value) = self.func_trigger.get_mut(key) {
                // Merge on_complete targets
                for target in &other_value.on_complete {
                    if !value.on_complete.contains(target) {
                        value.on_complete.push(target.clone());
                    }
                }

                // Merge on_error targets
                for target in &other_value.on_error {
                    if !value.on_error.contains(target) {
                        value.on_error.push(target.clone());
                    }
                }
            } else {
                // Key doesn't exist, insert the whole FuncTrigger
                self.func_trigger.insert(key.clone(), other_value.clone());
            }
        }

        // Merge data triggers
        for (key, other_value) in &other.data_trigger {
            if let Some(value) = self.data_trigger.get_mut(key) {
                // Merge on_create targets
                for target in &other_value.on_create {
                    if !value.on_create.contains(target) {
                        value.on_create.push(target.clone());
                    }
                }

                // Merge on_update targets
                for target in &other_value.on_update {
                    if !value.on_update.contains(target) {
                        value.on_update.push(target.clone());
                    }
                }

                // Merge on_delete targets
                for target in &other_value.on_delete {
                    if !value.on_delete.contains(target) {
                        value.on_delete.push(target.clone());
                    }
                }
            } else {
                // Key doesn't exist, insert the whole DataTrigger
                self.data_trigger.insert(*key, other_value.clone());
            }
        }
    }
}

use std::hash::{Hash, Hasher};
impl Hash for ObjectEvent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash function triggers
        for (key, value) in &self.func_trigger {
            key.hash(state);
            // Hash the targets in on_complete
            for target in &value.on_complete {
                target.hash(state);
            }
            // Hash the targets in on_error
            for target in &value.on_error {
                target.hash(state);
            }
        }

        // Hash data triggers
        for (key, value) in &self.data_trigger {
            key.hash(state);
            // Hash all target collections
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
