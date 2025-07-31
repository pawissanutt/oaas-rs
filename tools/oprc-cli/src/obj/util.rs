use std::{collections::HashMap, io::Read};

use oprc_pb::{ValData, ValType};

use crate::types::InvokeOperation;

pub fn parse_key_value_pairs(pairs: Vec<String>) -> HashMap<u32, ValData> {
    let mut map = HashMap::new();
    for kv in pairs {
        if let Some((key, value)) = kv.split_once('=') {
            match key.parse::<u32>() {
                Ok(parsed_key) => {
                    let b = value.as_bytes().to_vec();
                    let val = ValData {
                        data: b.into(),
                        r#type: ValType::Byte as i32,
                    };
                    map.insert(parsed_key, val);
                }
                Err(e) => {
                    eprintln!("Failed to parse key '{}': {}", key, e);
                }
            }
        } else {
            eprintln!("Invalid key-value format: {}", kv);
        }
    }
    map
}

pub fn extract_payload(opt: &InvokeOperation) -> Vec<u8> {
    let mut payload = Vec::new();
    if let Some(p) = &opt.payload {
        let mut reader =
            p.clone().into_reader().expect("Failed to create reader");
        reader
            .read_to_end(&mut payload)
            .expect("Failed to read payload");
    }
    payload
}
