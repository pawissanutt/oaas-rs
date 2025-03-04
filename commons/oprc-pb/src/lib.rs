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
