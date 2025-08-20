#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Package name cannot be empty")]
    EmptyPackageName,

    #[error("Duplicate class key: {0}")]
    DuplicateClassKey(String),

    #[error("Duplicate function key: {0}")]
    DuplicateFunctionKey(String),

    #[error("Dependency name cannot be empty")]
    EmptyDependencyName,

    #[error("Class key cannot be empty")]
    EmptyClassKey,

    #[error("Function bindings cannot be empty")]
    EmptyFunctionBindings,

    #[error("Invalid resource requirement: {0}")]
    InvalidResourceRequirement(String),

    #[error("Invalid NFR requirement: {0}")]
    InvalidNfrRequirement(String),

    #[error("Validator error: {0}")]
    ValidatorError(#[from] validator::ValidationErrors),
}
