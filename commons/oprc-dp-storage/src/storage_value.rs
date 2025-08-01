use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Optimized storage value type that uses stack allocation for small values
/// and zero-copy reference counting for large values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
}
