//! Environment configuration helpers

/// Check if dev mock mode is enabled (default: false)
/// Accepts common truthy values: "1", "true", "yes", "on" (case-insensitive)
#[cfg(feature = "server")]
pub fn is_dev_mock() -> bool {
    match std::env::var("OPRC_GUI_DEV_MOCK") {
        Ok(val) => {
            let v = val.to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

/// Get gateway base URL
#[cfg(feature = "server")]
pub fn gateway_base_url() -> String {
    std::env::var("OPRC_GATEWAY_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string())
}

/// Get PM base URL
#[cfg(feature = "server")]
pub fn pm_base_url() -> String {
    std::env::var("OPRC_PM_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string())
}
