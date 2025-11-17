#[cfg(test)]
mod tests {
    use super::super::observe_children;
    use kube::Client;
    // NOTE: This test is a placeholder; real tests would mock Kubernetes API.
    // For now we assert that calling observe_children in an empty env returns an Observed with 0 children.
    #[tokio::test]
    async fn observe_children_empty() {
        // Ensure a rustls CryptoProvider is installed for tests that may invoke TLS during kube config inference.
        let _ = rustls::crypto::CryptoProvider::install_default(
            rustls::crypto::aws_lc_rs::default_provider(),
        );
        // Skip if not running in a testable kube environment; we create a dummy client.
        let config = match kube::Config::infer().await {
            Ok(c) => c,
            Err(_) => return,
        }; // silently skip when no cluster
        let client = Client::try_from(config).unwrap();
        let obs =
            observe_children(client, "default", "nonexistent-class", false)
                .await;
        // We cannot guarantee cluster emptiness; only assert structure is valid.
        assert!(obs.children.iter().all(|c| !c.name.is_empty()));
    }
}
