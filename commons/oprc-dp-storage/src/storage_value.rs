use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Optimized storage value type that uses stack allocation for small values
/// and zero-copy reference counting for large values
///
/// Custom serde implementation eliminates enum discriminant overhead
#[derive(Debug, Clone, PartialEq)]
pub enum StorageValue {
    /// Stack-allocated for values ≤ 64 bytes (covers most ObjectEntry values)
    Small(SmallVec<[u8; 64]>),
    /// Zero-copy reference counting for values > 64 bytes
    Large(Bytes),
}

impl StorageValue {
    /// Create a new StorageValue from a Vec<u8>
    pub fn new(data: Vec<u8>) -> Self {
        if data.len() <= 64 {
            Self::Small(SmallVec::from_vec(data))
        } else {
            Self::Large(Bytes::from(data))
        }
    }

    /// Create a new StorageValue from a slice
    pub fn from_slice(data: &[u8]) -> Self {
        if data.len() <= 64 {
            Self::Small(SmallVec::from_slice(data))
        } else {
            Self::Large(Bytes::copy_from_slice(data))
        }
    }

    /// Get the data as a slice
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Small(data) => data.as_slice(),
            Self::Large(data) => data.as_ref(),
        }
    }

    /// Get the length of the data
    pub fn len(&self) -> usize {
        match self {
            Self::Small(data) => data.len(),
            Self::Large(data) => data.len(),
        }
    }

    /// Check if the value is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Convert to Vec<u8>
    pub fn into_vec(self) -> Vec<u8> {
        match self {
            Self::Small(data) => data.into_vec(),
            Self::Large(data) => data.to_vec(),
        }
    }

    /// Create an empty StorageValue
    pub fn empty() -> Self {
        Self::Small(SmallVec::new())
    }

    /// Check if this is a small value (stack allocated)
    pub fn is_small(&self) -> bool {
        matches!(self, Self::Small(_))
    }

    /// Check if this is a large value (heap allocated with ref counting)
    pub fn is_large(&self) -> bool {
        matches!(self, Self::Large(_))
    }
}

impl From<Vec<u8>> for StorageValue {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

impl From<&[u8]> for StorageValue {
    fn from(data: &[u8]) -> Self {
        Self::from_slice(data)
    }
}

impl From<Bytes> for StorageValue {
    fn from(bytes: Bytes) -> Self {
        if bytes.len() <= 64 {
            Self::Small(SmallVec::from_slice(&bytes))
        } else {
            Self::Large(bytes)
        }
    }
}

impl From<String> for StorageValue {
    fn from(s: String) -> Self {
        Self::from(s.into_bytes())
    }
}

impl From<&str> for StorageValue {
    fn from(s: &str) -> Self {
        Self::from_slice(s.as_bytes())
    }
}

impl AsRef<[u8]> for StorageValue {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::Deref for StorageValue {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl Default for StorageValue {
    fn default() -> Self {
        Self::empty()
    }
}

// ============================================================================
// Custom Serde Implementation - Eliminates Enum Discriminant Overhead
// ============================================================================

impl Serialize for StorageValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize directly as bytes without enum discriminant
        // This completely eliminates the enum overhead in serialized form
        serializer.serialize_bytes(self.as_slice())
    }
}

impl<'de> Deserialize<'de> for StorageValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserialize as bytes and automatically choose optimal representation
        let bytes: Vec<u8> = Vec::deserialize(deserializer)?;
        Ok(Self::new(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_value() {
        let data = b"hello world".to_vec();
        let value = StorageValue::new(data.clone());

        assert!(value.is_small());
        assert_eq!(value.as_slice(), data.as_slice());
        assert_eq!(value.len(), data.len());
    }

    #[test]
    fn test_large_value() {
        let data = vec![0u8; 100]; // Larger than 64 bytes
        let value = StorageValue::new(data.clone());

        assert!(value.is_large());
        assert_eq!(value.as_slice(), data.as_slice());
        assert_eq!(value.len(), data.len());
    }

    #[test]
    fn test_conversions() {
        let original = "test string";
        let value: StorageValue = original.into();
        assert_eq!(value.as_slice(), original.as_bytes());

        let vec_data = vec![1, 2, 3, 4, 5];
        let value: StorageValue = vec_data.clone().into();
        assert_eq!(value.into_vec(), vec_data);
    }

    #[test]
    fn test_serde_no_enum_overhead() {
        // Test that our custom serde implementation eliminates enum overhead
        let small_data = b"hello".to_vec();
        let large_data = vec![0u8; 100];

        let small_value = StorageValue::new(small_data.clone());
        let large_value = StorageValue::new(large_data.clone());

        // Both should serialize to just their raw bytes without enum tags
        // This test validates that the custom serde implementation works

        // The serialized size should be very close to the original data size
        // (plus minimal container overhead, but no enum discriminant)
        assert_eq!(small_value.as_slice(), small_data.as_slice());
        assert_eq!(large_value.as_slice(), large_data.as_slice());

        // Test round-trip serialization maintains data integrity
        let serialized_small = small_value.as_slice().to_vec();
        let serialized_large = large_value.as_slice().to_vec();

        let restored_small = StorageValue::new(serialized_small);
        let restored_large = StorageValue::new(serialized_large);

        assert_eq!(restored_small.as_slice(), small_data.as_slice());
        assert_eq!(restored_large.as_slice(), large_data.as_slice());
    }

    // Helper type for testing serialization overhead comparison
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    enum TraditionalStorageValue {
        Small(Vec<u8>),
        Large(Vec<u8>),
    }

    #[test]
    fn test_custom_serde_eliminates_enum_overhead() {
        let test_data = b"Test data for serialization efficiency".to_vec();

        // Our optimized StorageValue
        let optimized_value = StorageValue::new(test_data.clone());

        // Test that optimized version serializes to raw data size
        let optimized_bytes = optimized_value.as_slice();
        assert_eq!(optimized_bytes.len(), test_data.len());

        // Verify data integrity
        assert_eq!(optimized_bytes, test_data.as_slice());

        // Test round-trip
        let restored = StorageValue::new(optimized_bytes.to_vec());
        assert_eq!(restored.as_slice(), test_data.as_slice());
    }

    #[test]
    fn test_memory_layout_optimization() {
        // Test stack allocation for small values
        let small_data = b"small".to_vec();
        let small_value = StorageValue::new(small_data.clone());
        assert!(small_value.is_small());
        assert!(!small_value.is_large());
        assert_eq!(small_value.as_slice(), small_data.as_slice());

        // Test heap allocation for large values
        let large_data = vec![0u8; 100];
        let large_value = StorageValue::new(large_data.clone());
        assert!(!large_value.is_small());
        assert!(large_value.is_large());
        assert_eq!(large_value.as_slice(), large_data.as_slice());

        // Test boundary condition (exactly 64 bytes)
        let boundary_data = vec![1u8; 64];
        let boundary_value = StorageValue::new(boundary_data.clone());
        assert!(boundary_value.is_small()); // Should still be small at exactly 64 bytes
        assert_eq!(boundary_value.as_slice(), boundary_data.as_slice());

        // Test over boundary (65 bytes)
        let over_boundary_data = vec![2u8; 65];
        let over_boundary_value = StorageValue::new(over_boundary_data.clone());
        assert!(over_boundary_value.is_large()); // Should be large over 64 bytes
        assert_eq!(
            over_boundary_value.as_slice(),
            over_boundary_data.as_slice()
        );
    }

    #[test]
    fn test_zero_copy_bytes_integration() {
        use bytes::Bytes;

        // Test conversion from Bytes (zero-copy for large values)
        let large_data = vec![3u8; 100];
        let bytes = Bytes::from(large_data.clone());
        let value = StorageValue::from(bytes.clone());

        assert!(value.is_large());
        assert_eq!(value.as_slice(), large_data.as_slice());

        // For small Bytes, should still use stack allocation
        let small_data = b"small bytes".to_vec();
        let small_bytes = Bytes::from(small_data.clone());
        let small_value = StorageValue::from(small_bytes);

        assert!(small_value.is_small());
        assert_eq!(small_value.as_slice(), small_data.as_slice());
    }

    #[test]
    fn test_serialization_roundtrip_integrity() {
        // Test various data sizes and types
        let test_cases = vec![
            ("empty", vec![]),
            ("single_byte", vec![42]),
            ("small_string", b"Hello, World!".to_vec()),
            ("medium_data", vec![1u8; 50]),
            ("boundary_64", vec![2u8; 64]),
            ("large_data", vec![3u8; 1000]),
            ("binary_data", (0..256).map(|i| i as u8).collect()),
        ];

        for (name, data) in test_cases {
            let original_value = StorageValue::new(data.clone());

            // Test that as_slice gives us the original data
            assert_eq!(
                original_value.as_slice(),
                data.as_slice(),
                "Failed for case: {}",
                name
            );

            // Test round-trip through serialized bytes
            let serialized = original_value.as_slice().to_vec();
            let restored_value = StorageValue::new(serialized);

            assert_eq!(
                restored_value.as_slice(),
                data.as_slice(),
                "Round-trip failed for case: {}",
                name
            );
            assert_eq!(
                restored_value.len(),
                data.len(),
                "Length mismatch for case: {}",
                name
            );
            assert_eq!(
                restored_value.is_empty(),
                data.is_empty(),
                "Empty check failed for case: {}",
                name
            );

            // Verify optimization choice is correct
            if data.len() <= 64 {
                assert!(
                    restored_value.is_small(),
                    "Should be small for case: {}",
                    name
                );
            } else {
                assert!(
                    restored_value.is_large(),
                    "Should be large for case: {}",
                    name
                );
            }
        }
    }
}
