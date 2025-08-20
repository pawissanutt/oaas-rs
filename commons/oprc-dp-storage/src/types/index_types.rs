use crate::StorageValue;

/// Index key extractor function type
pub type IndexKeyExtractor =
    Box<dyn Fn(&[u8], &StorageValue) -> Vec<StorageValue> + Send + Sync>; // Changed Vec<Vec<u8>> to Vec<StorageValue>

/// Query types for secondary indexes
#[derive(Debug, Clone)]
pub enum IndexQuery {
    Exact(StorageValue), // Changed from Vec<u8> to StorageValue
    Range { 
        start: StorageValue, // Changed from Vec<u8> to StorageValue
        end: StorageValue    // Changed from Vec<u8> to StorageValue
    },
    Prefix(StorageValue), // Changed from Vec<u8> to StorageValue
}

impl IndexQuery {
    /// Create an exact match query
    pub fn exact<T: Into<StorageValue>>(value: T) -> Self {
        Self::Exact(value.into())
    }

    /// Create a range query
    pub fn range<T: Into<StorageValue>>(start: T, end: T) -> Self {
        Self::Range {
            start: start.into(),
            end: end.into(),
        }
    }

    /// Create a prefix query
    pub fn prefix<T: Into<StorageValue>>(prefix: T) -> Self {
        Self::Prefix(prefix.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_query_creation() {
        let exact = IndexQuery::exact("test_value");
        let range = IndexQuery::range("start", "end");
        let prefix = IndexQuery::prefix("prefix_");

        match exact {
            IndexQuery::Exact(val) => assert_eq!(val.as_slice(), b"test_value"),
            _ => panic!("Expected Exact query"),
        }

        match range {
            IndexQuery::Range { start, end } => {
                assert_eq!(start.as_slice(), b"start");
                assert_eq!(end.as_slice(), b"end");
            }
            _ => panic!("Expected Range query"),
        }

        match prefix {
            IndexQuery::Prefix(val) => assert_eq!(val.as_slice(), b"prefix_"),
            _ => panic!("Expected Prefix query"),
        }
    }

    #[test]
    fn test_index_key_extractor() {
        let extractor: IndexKeyExtractor = Box::new(|_key: &[u8], value: &StorageValue| {
            // Example: extract first 4 bytes as index key
            if value.len() >= 4 {
                vec![StorageValue::from_slice(&value.as_slice()[..4])]
            } else {
                vec![StorageValue::from_slice(value.as_slice())]
            }
        });

        let test_value = StorageValue::from("test_data_longer_than_4_bytes");
        let keys = extractor(b"some_key", &test_value);
        
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].as_slice(), b"test");
    }
}
