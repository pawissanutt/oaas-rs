#![allow(unsafe_op_in_unsafe_fn)]

wit_bindgen::generate!({
    world: "oaas-object",
    path: "../../data-plane/oprc-wasm/wit",
});

use exports::oaas::odgm::guest_object::{
    Guest, InvocationResponse, ObjectProxy,
};
use oaas::odgm::object_context;
use oaas::odgm::types::{KeyValue, ResponseStatus};

struct EchoObject;

impl Guest for EchoObject {
    fn on_invoke(
        self_proxy: ObjectProxy,
        function_name: String,
        payload: Option<Vec<u8>>,
        _headers: Vec<KeyValue>,
    ) -> InvocationResponse {
        match function_name.as_str() {
            "echo" => {
                // Stateless: echo back the payload
                InvocationResponse {
                    status: ResponseStatus::Okay,
                    payload,
                    headers: vec![KeyValue {
                        key: "content-type".to_string(),
                        value: "application/json".to_string(),
                    }],
                }
            }
            "transform" => {
                // Stateful: read "data" field, append suffix, write back
                Self::handle_transform(self_proxy, payload)
            }
            "get_ref" => {
                // Demonstrate object-ref usage
                let obj_ref = self_proxy.ref_();
                let ref_str = format!(
                    "{}/{}/{}",
                    obj_ref.cls, obj_ref.partition_id, obj_ref.object_id
                );
                InvocationResponse {
                    status: ResponseStatus::Okay,
                    payload: Some(ref_str.into_bytes()),
                    headers: vec![],
                }
            }
            "cross_access" => {
                // Demonstrate cross-object access via object-ref
                Self::handle_cross_access(payload)
            }
            _ => InvocationResponse {
                status: ResponseStatus::InvalidRequest,
                payload: Some(
                    format!("Unknown function: {}", function_name).into_bytes(),
                ),
                headers: vec![],
            },
        }
    }
}

impl EchoObject {
    fn handle_transform(
        self_proxy: ObjectProxy,
        _payload: Option<Vec<u8>>,
    ) -> InvocationResponse {
        // 1. Read the "data" field from self using the proxy
        let data_opt = match self_proxy.get("data") {
            Ok(opt) => opt,
            Err(e) => {
                return InvocationResponse {
                    status: ResponseStatus::SystemError,
                    payload: Some(format!("{:?}", e).into_bytes()),
                    headers: vec![],
                };
            }
        };

        if let Some(data_bytes) = data_opt {
            // 2. Transform data: append " - seen by wasm"
            let mut result = data_bytes;
            result.extend_from_slice(b" - seen by wasm");

            // 3. Write the transformed data back via self.set
            if let Err(e) = self_proxy.set("data", &result) {
                return InvocationResponse {
                    status: ResponseStatus::SystemError,
                    payload: Some(format!("{:?}", e).into_bytes()),
                    headers: vec![],
                };
            }

            InvocationResponse {
                status: ResponseStatus::Okay,
                payload: Some(result),
                headers: vec![],
            }
        } else {
            // Try legacy "_raw" key for backward compatibility with existing tests
            let raw_opt = match self_proxy.get("_raw") {
                Ok(opt) => opt,
                Err(e) => {
                    return InvocationResponse {
                        status: ResponseStatus::SystemError,
                        payload: Some(format!("{:?}", e).into_bytes()),
                        headers: vec![],
                    };
                }
            };

            if let Some(raw_bytes) = raw_opt {
                let mut result = raw_bytes;
                result.extend_from_slice(b" - seen by wasm");

                if let Err(e) = self_proxy.set("_raw", &result) {
                    return InvocationResponse {
                        status: ResponseStatus::SystemError,
                        payload: Some(format!("{:?}", e).into_bytes()),
                        headers: vec![],
                    };
                }

                InvocationResponse {
                    status: ResponseStatus::Okay,
                    payload: Some(result),
                    headers: vec![],
                }
            } else {
                InvocationResponse {
                    status: ResponseStatus::AppError,
                    payload: Some(
                        b"Object has no 'data' or '_raw' field".to_vec(),
                    ),
                    headers: vec![],
                }
            }
        }
    }

    fn handle_cross_access(payload: Option<Vec<u8>>) -> InvocationResponse {
        // Payload should contain an object-ref string like "cls/partition/id"
        let ref_str = match payload {
            Some(p) => match String::from_utf8(p) {
                Ok(s) => s,
                Err(_) => {
                    return InvocationResponse {
                        status: ResponseStatus::InvalidRequest,
                        payload: Some(b"Invalid UTF-8 in payload".to_vec()),
                        headers: vec![],
                    };
                }
            },
            None => {
                return InvocationResponse {
                    status: ResponseStatus::InvalidRequest,
                    payload: Some(
                        b"Payload required: object-ref string".to_vec(),
                    ),
                    headers: vec![],
                };
            }
        };

        // Get proxy to the referenced object
        let other_proxy = match object_context::object_by_str(&ref_str) {
            Ok(proxy) => proxy,
            Err(e) => {
                return InvocationResponse {
                    status: ResponseStatus::SystemError,
                    payload: Some(format!("{:?}", e).into_bytes()),
                    headers: vec![],
                };
            }
        };

        // Read a field from the other object
        let value = match other_proxy.get("data") {
            Ok(opt) => opt,
            Err(e) => {
                return InvocationResponse {
                    status: ResponseStatus::SystemError,
                    payload: Some(format!("{:?}", e).into_bytes()),
                    headers: vec![],
                };
            }
        };

        InvocationResponse {
            status: ResponseStatus::Okay,
            payload: value,
            headers: vec![KeyValue {
                key: "x-source-ref".to_string(),
                value: ref_str,
            }],
        }
    }
}

export!(EchoObject);
