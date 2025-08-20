//! Error types for MST replication

/// Simple error type for MST networking
#[derive(Debug, Clone)]
pub struct MstError(pub String);

impl std::fmt::Display for MstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MstError {}

impl From<String> for MstError {
    fn from(s: String) -> Self {
        MstError(s)
    }
}

impl From<&str> for MstError {
    fn from(s: &str) -> Self {
        MstError(s.to_string())
    }
}
