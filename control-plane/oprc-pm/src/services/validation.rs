use crate::errors::{PackageError, PackageManagerError};
use oprc_models::OPackage;
use tracing::info;

pub struct PackageValidator {
    // Future: Add validation rules, schemas, etc.
}

impl PackageValidator {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn validate(
        &self,
        package: &OPackage,
    ) -> Result<(), PackageManagerError> {
        info!("Validating package: {}", package.name);

        // Basic validation (beyond what's already done by the Validate trait)
        self.validate_package_structure(package)?;
        self.validate_class_function_consistency(package)?;
        self.validate_function_bindings(package)?;

        info!("Package validation passed: {}", package.name);
        Ok(())
    }

    fn validate_package_structure(
        &self,
        package: &OPackage,
    ) -> Result<(), PackageError> {
        // Check that package name is valid (no special characters, etc.)
        if package.name.is_empty() {
            return Err(PackageError::Invalid(
                "Package name cannot be empty".to_string(),
            ));
        }

        if package.name.contains(' ') {
            return Err(PackageError::Invalid(
                "Package name cannot contain spaces".to_string(),
            ));
        }

        // Check that all classes reference this package
        for class in &package.classes {
            if class.package != package.name {
                return Err(PackageError::Invalid(format!(
                    "Class {} references wrong package: {} (expected: {})",
                    class.key, class.package, package.name
                )));
            }
        }

        // Check that all functions reference this package
        for function in &package.functions {
            if function.package != package.name {
                return Err(PackageError::Invalid(format!(
                    "Function {} references wrong package: {} (expected: {})",
                    function.key, function.package, package.name
                )));
            }
        }

        Ok(())
    }

    fn validate_class_function_consistency(
        &self,
        package: &OPackage,
    ) -> Result<(), PackageError> {
        // Collect all function keys
        let function_keys: std::collections::HashSet<String> =
            package.functions.iter().map(|f| f.key.clone()).collect();

        // Check that all function bindings in classes reference existing functions
        for class in &package.classes {
            for binding in &class.functions {
                if !function_keys.contains(&binding.function_key) {
                    return Err(PackageError::Invalid(format!(
                        "Class {} references non-existent function: {}",
                        class.key, binding.function_key
                    )));
                }
            }
        }

        Ok(())
    }

    fn validate_function_bindings(
        &self,
        package: &OPackage,
    ) -> Result<(), PackageError> {
        for class in &package.classes {
            for binding in &class.functions {
                // Validate binding configuration
                if binding.function_key.is_empty() {
                    return Err(PackageError::Invalid(format!(
                        "Class {} has empty function key in binding",
                        class.key
                    )));
                }

                // TODO: Add more sophisticated validation:
                // - Check that runtime configurations are valid
                // - Validate resource requirements
                // - Check environment variable consistency
                // - Validate dependency injection configurations
            }
        }

        Ok(())
    }
}

impl Default for PackageValidator {
    fn default() -> Self {
        Self::new()
    }
}
