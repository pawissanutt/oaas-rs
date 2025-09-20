use crate::templates::manager::TemplateManager;

/// Predict the stable in-cluster Service name for a function index.
/// Single function -> `<base>`, multi -> `<base>-fn-<idx>`
#[inline]
pub fn function_service_name(
    class_name: &str,
    idx: usize,
    total: usize,
) -> String {
    let base = TemplateManager::dns1035_safe(class_name);
    if total > 1 {
        format!("{}-fn-{}", base, idx)
    } else {
        base
    }
}

/// Predict the HTTP URL for a function Service. Always uses port 80.
#[inline]
pub fn function_service_url(
    class_name: &str,
    idx: usize,
    total: usize,
) -> String {
    format!(
        "http://{}:80/",
        function_service_name(class_name, idx, total)
    )
}
