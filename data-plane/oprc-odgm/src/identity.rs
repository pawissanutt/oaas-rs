use lazy_static::lazy_static;
use regex::Regex;

#[derive(Debug, thiserror::Error)]
pub enum NormalizationError {
    #[error("empty id")]
    Empty,
    #[error("exceeds maximum length {max} (got {len})")]
    Length { len: usize, max: usize },
    #[error("invalid characters present")]
    InvalidChars,
    #[error("both numeric and string identifiers provided")]
    BothProvided,
    #[error("neither numeric nor string identifier provided")]
    NoneProvided,
}

/// Represents a validated object identity (numeric or normalized string).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ObjectIdentity {
    Numeric(u64),
    Str(String),
}

impl ObjectIdentity {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Numeric(_))
    }
    pub fn as_u64(&self) -> Option<u64> {
        if let Self::Numeric(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        if let Self::Str(s) = self {
            Some(s.as_str())
        } else {
            None
        }
    }
}

lazy_static! {
    static ref VALID_CHARS: Regex = Regex::new(r"^[a-z0-9._:-]+$").unwrap();
}

/// Normalize a raw string identifier:
/// - Lowercase ASCII
/// - Validate character set (restricted for initial implementation)
/// - Enforce max length
pub fn normalize_object_id(
    raw: &str,
    max_len: usize,
) -> Result<String, NormalizationError> {
    if raw.is_empty() {
        return Err(NormalizationError::Empty);
    }
    let lc = raw.to_ascii_lowercase();
    if lc.len() > max_len {
        return Err(NormalizationError::Length {
            len: lc.len(),
            max: max_len,
        });
    }
    if !VALID_CHARS.is_match(&lc) {
        return Err(NormalizationError::InvalidChars);
    }
    Ok(lc)
}

/// Normalize entry key (currently same rules as object id).
pub fn normalize_entry_key(
    raw: &str,
    max_len: usize,
) -> Result<String, NormalizationError> {
    normalize_object_id(raw, max_len)
}

/// Build ObjectIdentity from mutually exclusive numeric / string inputs.
pub fn build_identity(
    numeric: Option<u64>,
    string_id: Option<&str>,
    max_len: usize,
) -> Result<ObjectIdentity, NormalizationError> {
    match (numeric, string_id) {
        (Some(_), Some(_)) => Err(NormalizationError::BothProvided),
        (None, None) => Err(NormalizationError::NoneProvided),
        (Some(n), None) => Ok(ObjectIdentity::Numeric(n)),
        (None, Some(s)) => {
            Ok(ObjectIdentity::Str(normalize_object_id(s, max_len)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_ok() {
        let s = normalize_object_id("User-ABC_01", 160).unwrap();
        assert_eq!(&s, "user-abc_01");
    }

    #[test]
    fn test_invalid_chars() {
        assert!(matches!(
            normalize_object_id("Bad$", 160),
            Err(NormalizationError::InvalidChars)
        ));
    }

    #[test]
    fn test_length() {
        let too_long = "a".repeat(200);
        assert!(matches!(
            normalize_object_id(&too_long, 160),
            Err(NormalizationError::Length { .. })
        ));
    }
}
