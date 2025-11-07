//! Storage key encoding helpers for string-based IDs proposal.
//! Implements the layout described in the design docs:
//! Metadata record key:      <object_id_utf8><0x00><0x00>
//! Event config record key:  <object_id_utf8><0x00><0x01>
//! Entry (numeric)          : <object_id_utf8><0x00><0x10><u32_be>
//! Entry (string)           : <object_id_utf8><0x00><0x11><key_len_varint><key_utf8>
//!
//! NOTE: Earlier phases stored the full object blob at the metadata key for
//! string IDs. With granular storage, metadata is now persisted separately.

/// Legacy numeric objects still use an 8-byte big-endian u64 key (Phase 2 keeps this intact).
#[inline]
pub fn legacy_numeric_object_key(id: u64) -> [u8; 8] {
    id.to_be_bytes()
}

/// Build metadata key for a normalized string object ID.
pub fn string_object_meta_key(normalized_id: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(normalized_id.len() + 2);
    v.extend_from_slice(normalized_id.as_bytes());
    v.push(0x00); // terminator
    v.push(0x00); // record type: metadata
    v
}

/// Build event config key for a normalized string object ID.
pub fn string_object_event_config_key(normalized_id: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(normalized_id.len() + 2);
    v.extend_from_slice(normalized_id.as_bytes());
    v.push(0x00); // terminator
    v.push(0x01); // record type: event config
    v
}

/// Build view key for storing a full ObjectData blob for string IDs.
/// This is used by the Zenoh network layer for roundtrip get/set without
/// interfering with granular metadata/entries.
pub fn string_object_view_key(normalized_id: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(normalized_id.len() + 2);
    v.extend_from_slice(normalized_id.as_bytes());
    v.push(0x00); // terminator
    v.push(0x02); // record type: view blob
    v
}

/// Build numeric entry key (not yet used in Phase 2, reserved for granular storage later).
pub fn string_object_entry_key_numeric(
    normalized_id: &str,
    key: u32,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(normalized_id.len() + 1 + 1 + 4);
    v.extend_from_slice(normalized_id.as_bytes());
    v.push(0x00);
    v.push(0x10); // record type numeric entry
    v.extend_from_slice(&key.to_be_bytes());
    v
}

/// Build string entry key (not yet used in Phase 2).
pub fn string_object_entry_key_string(
    normalized_id: &str,
    key: &str,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(normalized_id.len() + 1 + 1 + 1 + key.len());
    v.extend_from_slice(normalized_id.as_bytes());
    v.push(0x00);
    v.push(0x11); // record type string entry
    encode_varint(key.len(), &mut v);
    v.extend_from_slice(key.as_bytes());
    v
}

/// Prefix for scanning all records of a given object (meta + entries).
pub fn string_object_prefix(normalized_id: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(normalized_id.len() + 1);
    v.extend_from_slice(normalized_id.as_bytes());
    v.push(0x00);
    v
}

/// Record variants parsed from a string-based object key.
#[derive(Debug, PartialEq, Eq)]
pub enum StringObjectRecord<'a> {
    Meta,                 // metadata record
    EventConfig,          // event configuration record
    NumericEntry(u32),    // numeric entry key
    StringEntry(&'a str), // string entry key slice
}

/// Parse a raw key (produced by this module) into (object_id, record_variant).
/// Returns None if the key does not match the string-object pattern.
pub fn parse_string_object_key<'a>(
    raw: &'a [u8],
) -> Option<(String, StringObjectRecord<'a>)> {
    // Find terminator 0x00
    let term = raw.iter().position(|b| *b == 0x00)?;
    if term + 1 >= raw.len() {
        return None;
    }
    let record_type = raw[term + 1];
    let object_id_bytes = &raw[..term];
    let object_id = String::from_utf8(object_id_bytes.to_vec()).ok()?;
    match record_type {
        0x00 => Some((object_id, StringObjectRecord::Meta)),
        0x01 => Some((object_id, StringObjectRecord::EventConfig)),
        0x10 => {
            if raw.len() < term + 2 + 4 {
                return None;
            }
            let mut arr = [0u8; 4];
            arr.copy_from_slice(&raw[term + 2..term + 6]);
            let num = u32::from_be_bytes(arr);
            Some((object_id, StringObjectRecord::NumericEntry(num)))
        }
        0x11 => {
            // varint starts at term+2
            let (len, consumed) = decode_varint(&raw[term + 2..])?;
            let start = term + 2 + consumed;
            let end = start + len;
            if end > raw.len() {
                return None;
            }
            let key_slice = std::str::from_utf8(&raw[start..end]).ok()?;
            Some((object_id, StringObjectRecord::StringEntry(key_slice)))
        }
        _ => None,
    }
}

/// Decode unsigned LEB128 varint; returns (value, bytes_consumed).
fn decode_varint(input: &[u8]) -> Option<(usize, usize)> {
    let mut shift = 0usize;
    let mut value = 0usize;
    for (i, b) in input.iter().enumerate() {
        let chunk = (b & 0x7F) as usize;
        value |= chunk << shift;
        if b & 0x80 == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
        if shift > 63 {
            return None;
        } // unreasonable
    }
    None
}

/// Simple LEB128-style unsigned varint encoder.
fn encode_varint(mut n: usize, out: &mut Vec<u8>) {
    while n >= 0x80 {
        out.push(((n as u8) & 0x7F) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::cmp::Ordering;

    #[test]
    fn test_string_meta_key() {
        let k = string_object_meta_key("abc");
        assert_eq!(k, vec![b'a', b'b', b'c', 0x00, 0x00]);
    }

    #[test]
    fn test_entry_key_numeric() {
        let k = string_object_entry_key_numeric("abc", 42);
        assert_eq!(&k[..3], b"abc");
        assert_eq!(k[3], 0x00);
        assert_eq!(k[4], 0x10);
        assert_eq!(&k[5..], 42u32.to_be_bytes());
    }

    #[test]
    fn test_entry_key_string() {
        let k = string_object_entry_key_string("abc", "f");
        assert_eq!(&k[..3], b"abc");
        assert_eq!(k[3], 0x00);
        assert_eq!(k[4], 0x11);
        // varint length for 1 byte key == 0x01
        assert_eq!(k[5], 0x01);
        assert_eq!(k[6], b'f');
    }

    #[test]
    fn test_ordering_meta_before_entries() {
        let meta = string_object_meta_key("obj");
        let e_num = string_object_entry_key_numeric("obj", 1);
        let e_str = string_object_entry_key_string("obj", "field");
        assert!(
            meta < e_num && meta < e_str,
            "metadata key must sort before entries"
        );
    }

    #[test]
    fn test_prefix_relationship() {
        let prefix = string_object_prefix("order-123");
        let meta = string_object_meta_key("order-123");
        let e = string_object_entry_key_string("order-123", "total");
        assert!(meta.starts_with(&prefix));
        assert!(e.starts_with(&prefix));
    }

    #[test]
    fn test_varint_multi_byte() {
        let s = string_object_entry_key_string("x", &"a".repeat(300));
        // after 'x' 0x00 0x11 => varint length starts at index 4
        // verify first varint byte high bit set, second not
        let _first = s[4 + 1]; // index: x(0) 0x00(1) 0x11(2) varint start(3) actually adjust: prefix length = 1 +1 +1 => 3
        // Recompute positions precisely:
        // bytes: [ 'x'(0), 0x00(1), 0x11(2), varint..., key... ]
        let first_var = s[3];
        let second_var = s[4];
        assert!(
            first_var & 0x80 != 0,
            "first varint byte should have continuation bit"
        );
        assert!(
            second_var & 0x80 == 0,
            "second varint byte should terminate"
        );
    }

    #[test]
    fn test_parse_meta() {
        let k = string_object_meta_key("order-1");
        let (id, rec) = parse_string_object_key(&k).unwrap();
        assert_eq!(id, "order-1");
        assert_eq!(rec, StringObjectRecord::Meta);
    }

    #[test]
    fn test_parse_numeric_entry() {
        let k = string_object_entry_key_numeric("obj", 9);
        let (id, rec) = parse_string_object_key(&k).unwrap();
        assert_eq!(id, "obj");
        assert_eq!(rec, StringObjectRecord::NumericEntry(9));
    }

    #[test]
    fn test_parse_string_entry() {
        let k = string_object_entry_key_string("obj", "field-x");
        let (id, rec) = parse_string_object_key(&k).unwrap();
        assert_eq!(id, "obj");
        match rec {
            StringObjectRecord::StringEntry(s) => assert_eq!(s, "field-x"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_parse_invalid() {
        // Missing terminator
        assert!(parse_string_object_key(b"abc").is_none());
        // Bad record type
        let mut k = string_object_meta_key("abc");
        *k.last_mut().unwrap() = 0xFF; // change record type
        assert!(parse_string_object_key(&k).is_none());
    }

    #[test]
    fn test_legacy_numeric_key_untouched() {
        let id = 0x0102_0304_0506_0708u64;
        let legacy = legacy_numeric_object_key(id);
        assert_eq!(
            legacy,
            id.to_be_bytes(),
            "legacy encoding must remain big-endian u64"
        );
    }

    #[test]
    fn test_fuzz_meta_key_roundtrip() {
        let charset: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789._:-";
        for _ in 0..500 {
            let len = rand::random_range(1..40);
            let s: String = (0..len)
                .map(|_| {
                    let idx = rand::random_range(0..charset.len());
                    charset[idx] as char
                })
                .collect();
            let key = string_object_meta_key(&s);
            let (parsed, rec) = parse_string_object_key(&key).expect("parse");
            assert_eq!(parsed, s);
            assert_eq!(rec, StringObjectRecord::Meta);
        }
    }
}
