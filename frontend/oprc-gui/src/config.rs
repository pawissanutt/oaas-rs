//! Environment configuration helpers

/// Check if dev mock mode is enabled
pub fn is_dev_mock() -> bool {
    std::env::var("OPRC_GUI_DEV_MOCK")
        .unwrap_or_else(|_| "true".to_string())
        .parse()
        .unwrap_or(true)
}

/// Get gateway base URL
pub fn gateway_base_url() -> String {
    std::env::var("OPRC_GATEWAY_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Get PM base URL
pub fn pm_base_url() -> String {
    std::env::var("OPRC_PM_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string())
}
