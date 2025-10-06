//! Granular (per-entry) storage key encoding and metadata structures.
//! Phase B: Key module and trait extensions for granular storage.
//!
//! This module extends the existing `storage_key` with additional utilities
//! for per-entry granular storage, including:
//! - Numeric key to string conversion
//! - Versioned metadata structures
//! - Entry key parsing and encoding helpers
//!
//! Key design: All entry keys are strings (numeric keys converted to decimal).
//! Storage layout: <object_id_utf8><0x00><record_type><key_string>

use crate::storage_key::{
    StringObjectRecord, parse_string_object_key,
    string_object_entry_key_string, string_object_meta_key,
    string_object_prefix,
};

/// Convert numeric entry key to canonical string representation.
/// Uses un-padded decimal ASCII for simplicity and smaller size for small numbers.
///
/// Examples:
/// - 0 → "0"
/// - 42 → "42"
/// - 1000 → "1000"
#[inline]
pub fn numeric_key_to_string(key: u32) -> String {
    key.to_string()
}

/// Try to parse a string key back to numeric form.
/// Returns Some(u32) if the string is a valid decimal number within u32 range.
#[inline]
pub fn string_to_numeric_key(key: &str) -> Option<u32> {
    key.parse::<u32>().ok()
}

/// Versioned object metadata persisted separately from entries.
/// This structure is stored at the metadata key: <object_id><0x00><0x00>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectMetadata {
    /// Monotonic version number, incremented once per batch mutation.
    pub object_version: u64,

    /// Optional tombstone flag (for future soft-delete support).
    pub tombstone: bool,

    /// Optional additional attributes (reserved for future use).
    pub attributes: Vec<(String, String)>,
}

impl Default for ObjectMetadata {
    fn default() -> Self {
        Self {
            object_version: 0,
            tombstone: false,
            attributes: Vec::new(),
        }
    }
}

impl ObjectMetadata {
    /// Create new metadata with version 0.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create metadata with specific version.
    pub fn with_version(version: u64) -> Self {
        Self {
            object_version: version,
            tombstone: false,
            attributes: Vec::new(),
        }
    }

    /// Increment object version (called on each batch mutation).
    pub fn increment_version(&mut self) {
        self.object_version = self.object_version.saturating_add(1);
    }

    /// Mark as tombstone.
    pub fn mark_tombstone(&mut self) {
        self.tombstone = true;
    }

    /// Serialize to bytes (simple bincode for now).
    pub fn to_bytes(&self) -> Vec<u8> {
        // Simple format: version(8) + tombstone(1) + attr_count(2) + attrs
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.object_version.to_le_bytes());
        bytes.push(if self.tombstone { 1 } else { 0 });
        bytes.extend_from_slice(&(self.attributes.len() as u16).to_le_bytes());
        for (k, v) in &self.attributes {
            bytes.extend_from_slice(&(k.len() as u16).to_le_bytes());
            bytes.extend_from_slice(k.as_bytes());
            bytes.extend_from_slice(&(v.len() as u16).to_le_bytes());
            bytes.extend_from_slice(v.as_bytes());
        }
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 11 {
            return None; // minimum: 8 (version) + 1 (tombstone) + 2 (count)
        }
        let mut version_bytes = [0u8; 8];
        version_bytes.copy_from_slice(&bytes[0..8]);
        let object_version = u64::from_le_bytes(version_bytes);
        let tombstone = bytes[8] != 0;

        let mut count_bytes = [0u8; 2];
        count_bytes.copy_from_slice(&bytes[9..11]);
        let count = u16::from_le_bytes(count_bytes) as usize;

        let mut attributes = Vec::with_capacity(count);
        let mut pos = 11;
        for _ in 0..count {
            if pos + 2 > bytes.len() {
                return None;
            }
            let mut klen_bytes = [0u8; 2];
            klen_bytes.copy_from_slice(&bytes[pos..pos + 2]);
            let klen = u16::from_le_bytes(klen_bytes) as usize;
            pos += 2;

            if pos + klen + 2 > bytes.len() {
                return None;
            }
            let key =
                String::from_utf8(bytes[pos..pos + klen].to_vec()).ok()?;
            pos += klen;

            let mut vlen_bytes = [0u8; 2];
            vlen_bytes.copy_from_slice(&bytes[pos..pos + 2]);
            let vlen = u16::from_le_bytes(vlen_bytes) as usize;
            pos += 2;

            if pos + vlen > bytes.len() {
                return None;
            }
            let value =
                String::from_utf8(bytes[pos..pos + vlen].to_vec()).ok()?;
            pos += vlen;

            attributes.push((key, value));
        }

        Some(Self {
            object_version,
            tombstone,
            attributes,
        })
    }
}

/// Build entry key for a string-based object with string entry key.
/// All numeric entry keys should be converted to strings first using `numeric_key_to_string`.
#[inline]
pub fn build_entry_key(normalized_object_id: &str, entry_key: &str) -> Vec<u8> {
    string_object_entry_key_string(normalized_object_id, entry_key)
}

/// Build metadata key for a string-based object.
#[inline]
pub fn build_metadata_key(normalized_object_id: &str) -> Vec<u8> {
    string_object_meta_key(normalized_object_id)
}

/// Build prefix for scanning all records of an object.
#[inline]
pub fn build_object_prefix(normalized_object_id: &str) -> Vec<u8> {
    string_object_prefix(normalized_object_id)
}

/// Parse result for granular storage keys.
#[derive(Debug, PartialEq, Eq)]
pub enum GranularRecord<'a> {
    /// Metadata record containing versioning info.
    Metadata,
    /// Event configuration record.
    EventConfig,
    /// Entry record with string key.
    Entry(&'a str),
}

/// Parse a granular storage key into (object_id, record_type).
/// Returns None if the key format is invalid.
pub fn parse_granular_key(raw: &[u8]) -> Option<(String, GranularRecord<'_>)> {
    let (object_id, record) = parse_string_object_key(raw)?;
    match record {
        StringObjectRecord::Meta => Some((object_id, GranularRecord::Metadata)),
        StringObjectRecord::EventConfig => {
            Some((object_id, GranularRecord::EventConfig))
        }
        StringObjectRecord::NumericEntry(_) => {
            // Convert numeric to string on the fly for backward compat during transition
            // In pure granular mode, numeric entries should not exist
            // For now, reject them to enforce string-only design
            None
        }
        StringObjectRecord::StringEntry(key) => {
            Some((object_id, GranularRecord::Entry(key)))
        }
    }
}

/// Helper to check if a key is a metadata key.
#[inline]
pub fn is_metadata_key(raw: &[u8]) -> bool {
    matches!(parse_granular_key(raw), Some((_, GranularRecord::Metadata)))
}

/// Helper to check if a key is an entry key.
#[inline]
pub fn is_entry_key(raw: &[u8]) -> bool {
    matches!(parse_granular_key(raw), Some((_, GranularRecord::Entry(_))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numeric_key_conversion() {
        assert_eq!(numeric_key_to_string(0), "0");
        assert_eq!(numeric_key_to_string(42), "42");
        assert_eq!(numeric_key_to_string(1000), "1000");

        assert_eq!(string_to_numeric_key("0"), Some(0));
        assert_eq!(string_to_numeric_key("42"), Some(42));
        assert_eq!(string_to_numeric_key("1000"), Some(1000));
        assert_eq!(string_to_numeric_key("abc"), None);
        assert_eq!(string_to_numeric_key("12.5"), None);
    }

    #[test]
    fn test_metadata_serialization() {
        let mut meta = ObjectMetadata::new();
        meta.object_version = 42;
        meta.tombstone = true;
        meta.attributes
            .push(("key1".to_string(), "val1".to_string()));

        let bytes = meta.to_bytes();
        let decoded = ObjectMetadata::from_bytes(&bytes).unwrap();

        assert_eq!(decoded.object_version, 42);
        assert_eq!(decoded.tombstone, true);
        assert_eq!(decoded.attributes.len(), 1);
        assert_eq!(decoded.attributes[0].0, "key1");
        assert_eq!(decoded.attributes[0].1, "val1");
    }

    #[test]
    fn test_metadata_default() {
        let meta = ObjectMetadata::default();
        assert_eq!(meta.object_version, 0);
        assert_eq!(meta.tombstone, false);
        assert!(meta.attributes.is_empty());
    }

    #[test]
    fn test_metadata_increment() {
        let mut meta = ObjectMetadata::new();
        assert_eq!(meta.object_version, 0);
        meta.increment_version();
        assert_eq!(meta.object_version, 1);
        meta.increment_version();
        assert_eq!(meta.object_version, 2);
    }

    #[test]
    fn test_build_entry_key() {
        let key = build_entry_key("obj-123", "field_name");
        // Should produce: "obj-123" + 0x00 + 0x11 + varint(len) + "field_name"
        assert!(key.starts_with(b"obj-123"));
        assert_eq!(key[7], 0x00); // terminator
        assert_eq!(key[8], 0x11); // string entry record type
    }

    #[test]
    fn test_parse_granular_key_metadata() {
        let key = build_metadata_key("test-obj");
        let (obj_id, rec) = parse_granular_key(&key).unwrap();
        assert_eq!(obj_id, "test-obj");
        assert_eq!(rec, GranularRecord::Metadata);
    }

    #[test]
    fn test_parse_granular_key_entry() {
        let key = build_entry_key("test-obj", "my_field");
        let (obj_id, rec) = parse_granular_key(&key).unwrap();
        assert_eq!(obj_id, "test-obj");
        match rec {
            GranularRecord::Entry(k) => assert_eq!(k, "my_field"),
            _ => panic!("expected Entry variant"),
        }
    }

    #[test]
    fn test_is_metadata_key() {
        let meta_key = build_metadata_key("obj");
        let entry_key = build_entry_key("obj", "field");
        assert!(is_metadata_key(&meta_key));
        assert!(!is_metadata_key(&entry_key));
    }

    #[test]
    fn test_is_entry_key() {
        let meta_key = build_metadata_key("obj");
        let entry_key = build_entry_key("obj", "field");
        assert!(!is_entry_key(&meta_key));
        assert!(is_entry_key(&entry_key));
    }

    #[test]
    fn test_metadata_roundtrip_empty_attrs() {
        let meta = ObjectMetadata::with_version(100);
        let bytes = meta.to_bytes();
        let decoded = ObjectMetadata::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, meta);
    }

    #[test]
    fn test_metadata_roundtrip_multiple_attrs() {
        let mut meta = ObjectMetadata::with_version(7);
        meta.attributes.push(("a".to_string(), "1".to_string()));
        meta.attributes.push(("b".to_string(), "2".to_string()));
        meta.attributes
            .push(("longer_key".to_string(), "longer_value".to_string()));

        let bytes = meta.to_bytes();
        let decoded = ObjectMetadata::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, meta);
    }

    #[test]
    fn test_key_ordering() {
        let meta = build_metadata_key("order-test");
        let entry1 = build_entry_key("order-test", "a");
        let entry2 = build_entry_key("order-test", "z");

        // Metadata should sort before entries
        assert!(meta < entry1);
        assert!(meta < entry2);

        // Entries should sort lexicographically
        assert!(entry1 < entry2);
    }
}
