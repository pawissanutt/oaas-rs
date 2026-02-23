#![allow(unsafe_op_in_unsafe_fn)]

wit_bindgen::generate!({
    world: "oaas-function",
    path: "../../data-plane/oprc-wasm/wit",
});

use exports::oaas::odgm::guest_function::{
    Guest, InvocationRequest, InvocationResponse,
};
use oaas::odgm::data_access::{get_object, set_object};
use oaas::odgm::types::{
    Entry, KeyValue, ObjData, ResponseStatus, ValData, ValType,
};

struct EchoFunction;

impl Guest for EchoFunction {
    fn invoke_fn(req: InvocationRequest) -> InvocationResponse {
        // Echo back the payload
        InvocationResponse {
            status: ResponseStatus::Okay,
            payload: req.payload,
            headers: vec![KeyValue {
                key: "content-type".to_string(),
                value: "application/json".to_string(),
            }],
        }
    }

    fn invoke_obj(req: InvocationRequest) -> InvocationResponse {
        let object_id = req.object_id.unwrap_or_else(|| "unknown".to_string());

        // 1. Read object
        let obj_opt =
            match get_object(&req.cls_id, req.partition_id, &object_id) {
                Ok(o) => o,
                Err(e) => {
                    return InvocationResponse {
                        status: ResponseStatus::SystemError,
                        payload: Some(format!("{:?}", e).into_bytes()),
                        headers: vec![],
                    };
                }
            };

        if let Some(obj) = obj_opt {
            let mut data = obj.entries[0].value.data.clone();

            // 2. Transfrom data (e.g. append " - seen by wasm")
            let mut extension = b" - seen by wasm".to_vec();
            data.append(&mut extension);

            // 3. Write object back
            let new_obj = ObjData {
                metadata: None,
                entries: vec![Entry {
                    key: "_raw".to_string(),
                    value: ValData {
                        data: data.clone(),
                        val_type: ValType::Byte,
                    },
                }],
            };

            if let Err(e) =
                set_object(&req.cls_id, req.partition_id, &object_id, &new_obj)
            {
                return InvocationResponse {
                    status: ResponseStatus::SystemError,
                    payload: Some(format!("{:?}", e).into_bytes()),
                    headers: vec![],
                };
            }

            InvocationResponse {
                status: ResponseStatus::Okay,
                payload: Some(data),
                headers: vec![],
            }
        } else {
            InvocationResponse {
                status: ResponseStatus::AppError,
                payload: Some(b"Object not found".to_vec()),
                headers: vec![],
            }
        }
    }
}

export!(EchoFunction);
