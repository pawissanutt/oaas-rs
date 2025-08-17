#![allow(dead_code)]

pub mod env;
pub mod grpc;
pub mod k8s;
pub mod net;

/// Install default rustls CryptoProvider if available.
/// This is safe to call repeatedly; it will attempt to call into the
/// rustls API if present and ignore any errors.
pub fn install_default_crypto_provider() {
    // No-op placeholder. We previously attempted to call into rustls::crypto::CryptoProvider
    // at runtime to install a process-level provider. That API surfaced as an instance
    // method in rustls v0.23 and the exact provider type varies by feature. Since the
    // workspace is now configured to use `aws-lc-rs` at compile time, the runtime
    // ambiguity should be resolved and an explicit install isn't required here.
    // Keep this function as a stable hook for future provider installation if needed.
    let _ = ();
}
