// System E2E test library
// Provides shared utilities for end-to-end testing of OaaS-RS on Kind clusters

pub mod setup;
pub mod cli;
pub mod k8s_helpers;

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Environment variable to skip cleanup for debugging purposes
pub const SKIP_CLEANUP_ENV: &str = "E2E_SKIP_CLEANUP";

/// Default cluster name for e2e tests
pub const CLUSTER_NAME: &str = "oaas-e2e";

/// Images that need to be loaded into Kind
pub const REQUIRED_IMAGES: &[&str] = &[
    "ghcr.io/pawissanutt/oaas-rs/pm",
    "ghcr.io/pawissanutt/oaas-rs/crm",
    "ghcr.io/pawissanutt/oaas-rs/gateway",
    "ghcr.io/pawissanutt/oaas-rs/odgm",
    "ghcr.io/pawissanutt/oaas-rs/router",
];

/// Get the repository root directory
pub fn repo_root() -> Result<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let root = PathBuf::from(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .context("Failed to determine repo root")?
        .to_path_buf();
    Ok(root)
}

/// Check if cleanup should be skipped (for debugging)
pub fn should_skip_cleanup() -> bool {
    std::env::var(SKIP_CLEANUP_ENV).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_root() {
        let root = repo_root().unwrap();
        assert!(root.join("Cargo.toml").exists());
        assert!(root.join("k8s").exists());
    }
}
