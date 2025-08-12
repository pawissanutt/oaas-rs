use oprc_pm::config::AppConfig;
use serial_test::serial;
use std::env;

#[tokio::test]
#[serial]
async fn test_config_loading_from_env() {
    // Set test environment variables
    unsafe {
        env::set_var("SERVER_HOST", "127.0.0.1");
        env::set_var("SERVER_PORT", "3000");
        env::set_var("STORAGE_TYPE", "memory");
    }

    let config =
        AppConfig::load_from_env().expect("Failed to load config from env");

    assert_eq!(config.server_host, "127.0.0.1");
    assert_eq!(config.server_port, 3000);
    assert_eq!(config.storage_type, "memory");

    // Clean up
    unsafe {
        env::remove_var("SERVER_HOST");
        env::remove_var("SERVER_PORT");
        env::remove_var("STORAGE_TYPE");
    }
}

#[tokio::test]
#[serial]
async fn test_default_config_values() {
    // Clear any existing environment variables that might interfere
    let vars_to_clear = vec![
        "SERVER_HOST",
        "SERVER_PORT",
        "STORAGE_TYPE",
        "LOG_LEVEL",
        "METRICS_ENABLED",
    ];

    for var in &vars_to_clear {
        unsafe {
            env::remove_var(var);
        }
    }

    let config = AppConfig::load_from_env()
        .expect("Failed to load config with defaults");

    assert_eq!(config.server_host, "0.0.0.0");
    assert_eq!(config.server_port, 8080);
    assert_eq!(config.storage_type, "memory");
    assert_eq!(config.log_level, "info");
    assert_eq!(config.metrics_enabled, true);
}

#[tokio::test]
#[serial]
async fn test_etcd_config_from_env() {
    // Set etcd-specific environment variables
    unsafe {
        env::set_var("STORAGE_TYPE", "etcd");
        env::set_var("ETCD_ENDPOINTS", "localhost:2379,localhost:2380");
        env::set_var("ETCD_KEY_PREFIX", "/test/pm");
        env::set_var("ETCD_USERNAME", "test_user");
        env::set_var("ETCD_PASSWORD", "test_pass");
        env::set_var("ETCD_TIMEOUT", "60");
    }

    let config =
        AppConfig::load_from_env().expect("Failed to load etcd config from env");

    assert_eq!(config.storage_type, "etcd");
    assert_eq!(config.etcd_endpoints, "localhost:2379,localhost:2380");
    assert_eq!(config.etcd_key_prefix, "/test/pm");
    assert_eq!(config.etcd_username, Some("test_user".to_string()));
    assert_eq!(config.etcd_password, Some("test_pass".to_string()));
    assert_eq!(config.etcd_timeout_seconds, 60);

    // Clean up
    unsafe {
        env::remove_var("STORAGE_TYPE");
        env::remove_var("ETCD_ENDPOINTS");
        env::remove_var("ETCD_KEY_PREFIX");
        env::remove_var("ETCD_USERNAME");
        env::remove_var("ETCD_PASSWORD");
        env::remove_var("ETCD_TIMEOUT");
    }
}

#[tokio::test]
#[serial]
async fn test_crm_config_from_env() {
    // Set CRM-specific environment variables
    unsafe {
        env::set_var("CRM_DEFAULT_URL", "http://localhost:8081");
        env::set_var("CRM_DEFAULT_TIMEOUT", "45");
        env::set_var("CRM_DEFAULT_RETRY_ATTEMPTS", "5");
        env::set_var("CRM_HEALTH_CHECK_INTERVAL", "30");
    }

    let config =
        AppConfig::load_from_env().expect("Failed to load CRM config from env");

    assert_eq!(config.crm_default_url, Some("http://localhost:8081".to_string()));
    assert_eq!(config.crm_default_timeout_seconds, 45);
    assert_eq!(config.crm_default_retry_attempts, 5);
    assert_eq!(config.crm_health_check_interval_seconds, 30);

    // Clean up
    unsafe {
        env::remove_var("CRM_DEFAULT_URL");
        env::remove_var("CRM_DEFAULT_TIMEOUT");
        env::remove_var("CRM_DEFAULT_RETRY_ATTEMPTS");
        env::remove_var("CRM_HEALTH_CHECK_INTERVAL");
    }
}
