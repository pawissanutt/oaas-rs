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

/// Predict HTTP URL tailored for Knative (omit explicit :80 since Knative Service virtual service listens on 80).
#[inline]
pub fn function_service_url_knative(
    class_name: &str,
    idx: usize,
    total: usize,
) -> String {
    format!("http://{}/", function_service_name(class_name, idx, total))
}

/// Predict the fully-qualified in-cluster DNS URL (Deployment path) e.g. http://svc.ns.svc.cluster.local/ .
#[inline]
pub fn function_service_url_fqdn(
    class_name: &str,
    namespace: &str,
    idx: usize,
    total: usize,
) -> String {
    format!(
        "http://{}.{}.svc.cluster.local/",
        function_service_name(class_name, idx, total),
        namespace
    )
}

/// Predict fully-qualified Knative URL (cluster-local) http://svc.namespace.svc.cluster.local/ .
#[inline]
pub fn function_service_url_knative_fqdn(
    class_name: &str,
    namespace: &str,
    idx: usize,
    total: usize,
) -> String {
    // For cluster-local visibility; public domain URLs may differ (ingress configured)
    function_service_url_fqdn(class_name, namespace, idx, total)
}
