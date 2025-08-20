use crate::commands::{context, package};
use crate::config::{CliConfig, ContextConfig, ContextManager};
use crate::types::{ContextOperation, PackageOperation};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio;

// Helper function to create a temporary config directory
async fn create_test_config() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();

    // Set up test config
    let mut contexts = HashMap::new();
    contexts.insert(
        "test".to_string(),
        ContextConfig {
            pm_url: Some("http://test.pm.com".to_string()),
            gateway_url: Some("http://test.gateway.com".to_string()),
            default_class: Some("test.class".to_string()),
            zenoh_peer: Some("tcp/192.168.1.100:7447".to_string()),
        },
    );

    let config = CliConfig {
        contexts,
        current_context: "test".to_string(),
    };

    // Save config to temp directory
    let config_path = temp_dir.path().join("config.yml");
    let config_content = serde_yaml::to_string(&config).unwrap();
    tokio::fs::write(&config_path, config_content)
        .await
        .unwrap();

    (temp_dir, config_path)
}

#[test_log::test(tokio::test)]
async fn test_context_get_command() {
    let (_temp_dir, config_path) = create_test_config().await;
    let mut manager = ContextManager::with_config_path(&config_path)
        .await
        .unwrap();

    let operation = ContextOperation::Get;
    let result =
        context::handle_context_command_with_manager(&operation, &mut manager)
            .await;

    assert!(result.is_ok(), "Context get command should succeed");
}

#[test_log::test(tokio::test)]
async fn test_context_set_command() {
    let (_temp_dir, config_path) = create_test_config().await;
    let mut manager = ContextManager::with_config_path(&config_path)
        .await
        .unwrap();

    let operation = ContextOperation::Set {
        name: Some("integration_test".to_string()),
        pm: Some("http://integration.pm.com".to_string()),
        gateway: Some("http://integration.gateway.com".to_string()),
        cls: Some("integration.class".to_string()),
        zenoh_peer: Some("tcp/integration.host:7447".to_string()),
    };

    let result =
        context::handle_context_command_with_manager(&operation, &mut manager)
            .await;
    assert!(result.is_ok(), "Context set command should succeed");

    // Verify the context was created
    let context = manager.config().get_context("integration_test");
    assert!(context.is_some(), "New context should be created");

    let context = context.unwrap();
    assert_eq!(
        context.pm_url.as_ref().unwrap(),
        "http://integration.pm.com"
    );
    assert_eq!(
        context.gateway_url.as_ref().unwrap(),
        "http://integration.gateway.com"
    );
    assert_eq!(context.default_class.as_ref().unwrap(), "integration.class");
    assert_eq!(
        context.zenoh_peer.as_ref().unwrap(),
        "tcp/integration.host:7447"
    );
}

#[test_log::test(tokio::test)]
async fn test_context_select_command() {
    let (_temp_dir, config_path) = create_test_config().await;
    let mut manager = ContextManager::with_config_path(&config_path)
        .await
        .unwrap();

    // First create a new context
    let set_operation = ContextOperation::Set {
        name: Some("selectable_test".to_string()),
        pm: Some("http://selectable.pm.com".to_string()),
        gateway: Some("http://selectable.gateway.com".to_string()),
        cls: Some("selectable.class".to_string()),
        zenoh_peer: None,
    };
    context::handle_context_command_with_manager(&set_operation, &mut manager)
        .await
        .unwrap();

    // Now select it
    let select_operation = ContextOperation::Select {
        name: "selectable_test".to_string(),
    };

    let result = context::handle_context_command_with_manager(
        &select_operation,
        &mut manager,
    )
    .await;
    assert!(result.is_ok(), "Context select command should succeed");

    // Verify the context was selected
    assert_eq!(manager.config().current_context, "selectable_test");
}

#[test_log::test(tokio::test)]
async fn test_package_apply_command_file_not_found() {
    let (_temp_dir, _config_path) = create_test_config().await;

    let operation = PackageOperation::Apply {
        file: PathBuf::from("nonexistent.yaml"),
        override_package: None,
    };

    let result = package::handle_package_command(&operation).await;
    assert!(
        result.is_err(),
        "Package apply should fail for nonexistent file"
    );

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("not found"),
        "Error should mention file not found"
    );
}

#[test_log::test(tokio::test)]
async fn test_package_apply_command_with_valid_yaml() {
    let (_temp_dir, _config_path) = create_test_config().await;
    let temp_file_dir = TempDir::new().unwrap();

    // Create a test YAML file
    let yaml_content = r#"
package: test-package
version: 1.0.0
classes:
  - class: test_class
    functions:
      - function: test_function
        code: |
          pub fn test_function() -> String {
              "Hello, World!".to_string()
          }
"#;

    let yaml_path = temp_file_dir.path().join("test-package.yaml");
    tokio::fs::write(&yaml_path, yaml_content).await.unwrap();

    let operation = PackageOperation::Apply {
        file: yaml_path,
        override_package: None,
    };

    // This will fail due to no actual HTTP server, but should parse the YAML correctly
    let result = package::handle_package_command(&operation).await;

    // The command should fail at HTTP request stage, not YAML parsing
    if let Err(e) = result {
        let error_msg = e.to_string();
        // Should not be a YAML parsing error
        assert!(
            !error_msg.contains("YAML"),
            "Error should not be YAML parsing related: {}",
            error_msg
        );
    }
}

#[test_log::test(tokio::test)]
async fn test_package_apply_with_override() {
    let (_temp_dir, _config_path) = create_test_config().await;
    let temp_file_dir = TempDir::new().unwrap();

    // Create a test YAML file
    let yaml_content = r#"
package: original-package
version: 1.0.0
classes:
  - class: test_class
    functions:
      - function: test_function
        code: "pub fn test() {}"
"#;

    let yaml_path = temp_file_dir.path().join("test-package.yaml");
    tokio::fs::write(&yaml_path, yaml_content).await.unwrap();

    let operation = PackageOperation::Apply {
        file: yaml_path,
        override_package: Some("overridden-package".to_string()),
    };

    // This will fail due to no actual HTTP server, but should parse the YAML and apply override
    let result = package::handle_package_command(&operation).await;

    // The command should fail at HTTP request stage, not YAML parsing or override logic
    if let Err(e) = result {
        let error_msg = e.to_string();
        // Should not be a YAML parsing error
        assert!(
            !error_msg.contains("YAML"),
            "Error should not be YAML parsing related: {}",
            error_msg
        );
        assert!(
            !error_msg.contains("override"),
            "Error should not be override related: {}",
            error_msg
        );
    }
}

#[test_log::test(tokio::test)]
async fn test_package_delete_command() {
    let (_temp_dir, _config_path) = create_test_config().await;
    let temp_file_dir = TempDir::new().unwrap();

    // Create a test YAML file
    let yaml_content = r#"
package: delete-test-package
version: 1.0.0
classes: []
"#;

    let yaml_path = temp_file_dir.path().join("delete-test.yaml");
    tokio::fs::write(&yaml_path, yaml_content).await.unwrap();

    let operation = PackageOperation::Delete {
        file: yaml_path,
        override_package: None,
    };

    // This will fail due to no actual HTTP server, but should parse the YAML correctly
    let result = package::handle_package_command(&operation).await;

    // The command should fail at HTTP request stage, not YAML parsing
    if let Err(e) = result {
        let error_msg = e.to_string();
        // Should not be a YAML parsing error
        assert!(
            !error_msg.contains("YAML"),
            "Error should not be YAML parsing related: {}",
            error_msg
        );
    }
}

#[test]
fn test_context_operation_variants() {
    // Test that we can construct all ContextOperation variants
    let _get = ContextOperation::Get;
    let _set = ContextOperation::Set {
        name: Some("test".to_string()),
        pm: Some("http://test.com".to_string()),
        gateway: Some("http://gateway.com".to_string()),
        cls: Some("test.class".to_string()),
        zenoh_peer: Some("tcp/test:7447".to_string()),
    };
    let _select = ContextOperation::Select {
        name: "test".to_string(),
    };
}

#[test]
fn test_package_operation_variants() {
    // Test that we can construct all PackageOperation variants
    let _apply = PackageOperation::Apply {
        file: PathBuf::from("test.yaml"),
        override_package: Some("test-pkg".to_string()),
    };
    let _delete = PackageOperation::Delete {
        file: PathBuf::from("test.yaml"),
        override_package: None,
    };
}
