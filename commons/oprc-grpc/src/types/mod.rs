// Re-export common protobuf types for convenience
pub use crate::proto::{
    common::*, deployment::*, health::*, package::*, runtime::*,
};

// Validation error types
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Package name cannot be empty")]
    EmptyPackageName,

    #[error("Duplicate class key: {0}")]
    DuplicateClassKey(String),

    #[error("Dependency name cannot be empty")]
    EmptyDependencyName,

    #[error("Class key cannot be empty")]
    EmptyClassKey,

    #[error("Function bindings cannot be empty")]
    EmptyFunctionBindings,
}
