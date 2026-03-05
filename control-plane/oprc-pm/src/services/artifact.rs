//! Artifact storage module — stores compiled WASM modules and source code.
//!
//! Uses content-hash (SHA-256) addressing for deduplication.
//! The default backend writes files to a configurable directory on the filesystem.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs;
use tracing::{debug, info, warn};

/// Errors specific to artifact operations.
#[derive(Error, Debug)]
pub enum ArtifactError {
    #[error("Artifact not found: {0}")]
    NotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid artifact ID: {0}")]
    InvalidId(String),
}

/// Metadata about a stored artifact.
#[derive(Debug, Clone, Serialize)]
pub struct ArtifactInfo {
    pub id: String,
    pub size: u64,
    /// Optional build metadata (package, class, build time, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ArtifactMeta>,
}

/// Build metadata stored alongside an artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMeta {
    pub package: String,
    pub class_key: String,
    /// ISO-8601 build timestamp.
    pub built_at: String,
}

/// Metadata about a stored source file.
#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub package: String,
    pub function: String,
}

/// Trait for storing and retrieving compiled WASM artifacts.
#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Store artifact bytes, returning a content-hash ID.
    async fn store(&self, bytes: &[u8]) -> Result<String, ArtifactError>;

    /// Get artifact bytes by content-hash ID.
    async fn get(&self, id: &str) -> Result<Vec<u8>, ArtifactError>;

    /// Delete an artifact by content-hash ID. Returns true if it existed.
    async fn delete(&self, id: &str) -> Result<bool, ArtifactError>;

    /// Check if an artifact exists.
    async fn exists(&self, id: &str) -> Result<bool, ArtifactError>;

    /// List all stored artifacts with metadata.
    async fn list(&self) -> Result<Vec<ArtifactInfo>, ArtifactError>;

    /// Store build metadata for an artifact.
    async fn store_meta(
        &self,
        id: &str,
        meta: &ArtifactMeta,
    ) -> Result<(), ArtifactError>;

    /// Get build metadata for an artifact.
    async fn get_meta(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactMeta>, ArtifactError>;
}

/// Trait for storing and retrieving source code alongside artifacts.
#[async_trait]
pub trait SourceStore: Send + Sync {
    /// Store source code for a given package + function key.
    async fn store_source(
        &self,
        package: &str,
        function: &str,
        source: &str,
    ) -> Result<(), ArtifactError>;

    /// Get stored source code for a given package + function key.
    async fn get_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<Option<String>, ArtifactError>;

    /// Delete stored source code for a given package + function key.
    async fn delete_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<bool, ArtifactError>;

    /// Delete all source code for a given package.
    async fn delete_package_sources(
        &self,
        package: &str,
    ) -> Result<u32, ArtifactError>;

    /// List all stored sources.
    async fn list_sources(&self) -> Result<Vec<SourceInfo>, ArtifactError>;
}

/// Compute SHA-256 content hash of a byte slice, returned as hex string.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Validate that an artifact ID is a valid hex SHA-256 hash.
fn validate_id(id: &str) -> Result<(), ArtifactError> {
    if id.len() != 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ArtifactError::InvalidId(id.to_string()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Filesystem backend
// ---------------------------------------------------------------------------

/// Filesystem-backed artifact store.
///
/// Artifacts are written to `{base_dir}/wasm-modules/{content-hash}.wasm`.
/// Source code is written to `{base_dir}/sources/{package}/{function}.ts`.
pub struct FsArtifactStore {
    wasm_dir: PathBuf,
    source_dir: PathBuf,
}

impl FsArtifactStore {
    /// Create a new store rooted at `base_dir`. Creates directories if needed.
    pub async fn new(
        base_dir: impl AsRef<Path>,
    ) -> Result<Self, ArtifactError> {
        let wasm_dir = base_dir.as_ref().join("wasm-modules");
        let source_dir = base_dir.as_ref().join("sources");
        fs::create_dir_all(&wasm_dir).await?;
        fs::create_dir_all(&source_dir).await?;
        info!(
            wasm_dir = %wasm_dir.display(),
            source_dir = %source_dir.display(),
            "Artifact store initialized"
        );
        Ok(Self {
            wasm_dir,
            source_dir,
        })
    }

    fn wasm_path(&self, id: &str) -> PathBuf {
        self.wasm_dir.join(format!("{}.wasm", id))
    }

    fn meta_path(&self, id: &str) -> PathBuf {
        self.wasm_dir.join(format!("{}.meta.json", id))
    }

    fn source_path(&self, package: &str, function: &str) -> PathBuf {
        self.source_dir
            .join(package)
            .join(format!("{}.ts", function))
    }

    /// Return the base wasm-modules directory (for serving via HTTP).
    pub fn wasm_dir(&self) -> &Path {
        &self.wasm_dir
    }
}

#[async_trait]
impl ArtifactStore for FsArtifactStore {
    async fn store(&self, bytes: &[u8]) -> Result<String, ArtifactError> {
        let id = content_hash(bytes);
        let path = self.wasm_path(&id);
        if path.exists() {
            debug!(id = %id, "Artifact already exists (dedup hit)");
            return Ok(id);
        }
        // Write to temp file then rename for atomicity
        let tmp_path = path.with_extension("wasm.tmp");
        fs::write(&tmp_path, bytes).await?;
        fs::rename(&tmp_path, &path).await?;
        info!(id = %id, size = bytes.len(), "Artifact stored");
        Ok(id)
    }

    async fn get(&self, id: &str) -> Result<Vec<u8>, ArtifactError> {
        validate_id(id)?;
        let path = self.wasm_path(id);
        if !path.exists() {
            return Err(ArtifactError::NotFound(id.to_string()));
        }
        let bytes = fs::read(&path).await?;
        debug!(id = %id, size = bytes.len(), "Artifact read");
        Ok(bytes)
    }

    async fn delete(&self, id: &str) -> Result<bool, ArtifactError> {
        validate_id(id)?;
        let path = self.wasm_path(id);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path).await?;
        info!(id = %id, "Artifact deleted");
        Ok(true)
    }

    async fn exists(&self, id: &str) -> Result<bool, ArtifactError> {
        validate_id(id)?;
        Ok(self.wasm_path(id).exists())
    }

    async fn list(&self) -> Result<Vec<ArtifactInfo>, ArtifactError> {
        let mut artifacts = Vec::new();
        let mut entries = fs::read_dir(&self.wasm_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wasm") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.len() == 64
                        && stem.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        let file_meta = entry.metadata().await?;
                        // Try to read sidecar metadata
                        let artifact_meta = self.get_meta(stem).await.ok().flatten();
                        artifacts.push(ArtifactInfo {
                            id: stem.to_string(),
                            size: file_meta.len(),
                            meta: artifact_meta,
                        });
                    }
                }
            }
        }
        artifacts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(artifacts)
    }

    async fn store_meta(
        &self,
        id: &str,
        meta: &ArtifactMeta,
    ) -> Result<(), ArtifactError> {
        let path = self.meta_path(id);
        let json = serde_json::to_string_pretty(meta)
            .map_err(|e| ArtifactError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        fs::write(&path, json).await?;
        debug!(id = %id, "Artifact metadata stored");
        Ok(())
    }

    async fn get_meta(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactMeta>, ArtifactError> {
        let path = self.meta_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let json = fs::read_to_string(&path).await?;
        let meta: ArtifactMeta = serde_json::from_str(&json)
            .map_err(|e| ArtifactError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(Some(meta))
    }
}

#[async_trait]
impl SourceStore for FsArtifactStore {
    async fn store_source(
        &self,
        package: &str,
        function: &str,
        source: &str,
    ) -> Result<(), ArtifactError> {
        let path = self.source_path(package, function);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, source).await?;
        debug!(package = %package, function = %function, "Source stored");
        Ok(())
    }

    async fn get_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<Option<String>, ArtifactError> {
        let path = self.source_path(package, function);
        if !path.exists() {
            return Ok(None);
        }
        let source = fs::read_to_string(&path).await?;
        Ok(Some(source))
    }

    async fn delete_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<bool, ArtifactError> {
        let path = self.source_path(package, function);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path).await?;
        debug!(package = %package, function = %function, "Source deleted");
        Ok(true)
    }

    async fn delete_package_sources(
        &self,
        package: &str,
    ) -> Result<u32, ArtifactError> {
        let dir = self.source_dir.join(package);
        if !dir.exists() {
            return Ok(0);
        }
        let mut count = 0u32;
        let mut entries = fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry
                .file_type()
                .await
                .map(|ft| ft.is_file())
                .unwrap_or(false)
            {
                fs::remove_file(entry.path()).await?;
                count += 1;
            }
        }
        // Try to remove the (now empty) directory
        if let Err(e) = fs::remove_dir(&dir).await {
            warn!(package = %package, error = %e, "Could not remove package source dir");
        }
        info!(package = %package, count = count, "Package sources deleted");
        Ok(count)
    }

    async fn list_sources(&self) -> Result<Vec<SourceInfo>, ArtifactError> {
        let mut sources = Vec::new();
        let mut pkg_entries = fs::read_dir(&self.source_dir).await?;
        while let Some(pkg_entry) = pkg_entries.next_entry().await? {
            if pkg_entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false)
            {
                let package =
                    pkg_entry.file_name().to_string_lossy().to_string();
                let mut fn_entries = fs::read_dir(pkg_entry.path()).await?;
                while let Some(fn_entry) = fn_entries.next_entry().await? {
                    if let Some(stem) = fn_entry
                        .path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.to_string())
                    {
                        sources.push(SourceInfo {
                            package: package.clone(),
                            function: stem,
                        });
                    }
                }
            }
        }
        sources.sort_by(|a, b| {
            a.package.cmp(&b.package).then(a.function.cmp(&b.function))
        });
        Ok(sources)
    }
}

// ---------------------------------------------------------------------------
// In-memory backend (for tests)
// ---------------------------------------------------------------------------

/// In-memory artifact + source store for testing.
#[derive(Default)]
pub struct MemoryArtifactStore {
    wasm: tokio::sync::RwLock<std::collections::HashMap<String, Vec<u8>>>,
    metas: tokio::sync::RwLock<std::collections::HashMap<String, ArtifactMeta>>,
    sources: tokio::sync::RwLock<std::collections::HashMap<String, String>>,
}

impl MemoryArtifactStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn source_key(package: &str, function: &str) -> String {
        format!("{}/{}", package, function)
    }
}

#[async_trait]
impl ArtifactStore for MemoryArtifactStore {
    async fn store(&self, bytes: &[u8]) -> Result<String, ArtifactError> {
        let id = content_hash(bytes);
        self.wasm.write().await.insert(id.clone(), bytes.to_vec());
        Ok(id)
    }

    async fn get(&self, id: &str) -> Result<Vec<u8>, ArtifactError> {
        validate_id(id)?;
        self.wasm
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| ArtifactError::NotFound(id.to_string()))
    }

    async fn delete(&self, id: &str) -> Result<bool, ArtifactError> {
        validate_id(id)?;
        Ok(self.wasm.write().await.remove(id).is_some())
    }

    async fn exists(&self, id: &str) -> Result<bool, ArtifactError> {
        validate_id(id)?;
        Ok(self.wasm.read().await.contains_key(id))
    }

    async fn list(&self) -> Result<Vec<ArtifactInfo>, ArtifactError> {
        let wasm = self.wasm.read().await;
        let metas = self.metas.read().await;
        let mut artifacts: Vec<ArtifactInfo> = wasm
            .iter()
            .map(|(id, bytes)| ArtifactInfo {
                id: id.clone(),
                size: bytes.len() as u64,
                meta: metas.get(id).cloned(),
            })
            .collect();
        artifacts.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(artifacts)
    }

    async fn store_meta(
        &self,
        id: &str,
        meta: &ArtifactMeta,
    ) -> Result<(), ArtifactError> {
        self.metas
            .write()
            .await
            .insert(id.to_string(), meta.clone());
        Ok(())
    }

    async fn get_meta(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactMeta>, ArtifactError> {
        Ok(self.metas.read().await.get(id).cloned())
    }
}

#[async_trait]
impl SourceStore for MemoryArtifactStore {
    async fn store_source(
        &self,
        package: &str,
        function: &str,
        source: &str,
    ) -> Result<(), ArtifactError> {
        let key = Self::source_key(package, function);
        self.sources.write().await.insert(key, source.to_string());
        Ok(())
    }

    async fn get_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<Option<String>, ArtifactError> {
        let key = Self::source_key(package, function);
        Ok(self.sources.read().await.get(&key).cloned())
    }

    async fn delete_source(
        &self,
        package: &str,
        function: &str,
    ) -> Result<bool, ArtifactError> {
        let key = Self::source_key(package, function);
        Ok(self.sources.write().await.remove(&key).is_some())
    }

    async fn delete_package_sources(
        &self,
        package: &str,
    ) -> Result<u32, ArtifactError> {
        let prefix = format!("{}/", package);
        let mut sources = self.sources.write().await;
        let keys: Vec<String> = sources
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        let count = keys.len() as u32;
        for key in keys {
            sources.remove(&key);
        }
        Ok(count)
    }

    async fn list_sources(&self) -> Result<Vec<SourceInfo>, ArtifactError> {
        let sources = self.sources.read().await;
        let mut result: Vec<SourceInfo> = sources
            .keys()
            .filter_map(|key| {
                let parts: Vec<&str> = key.splitn(2, '/').collect();
                if parts.len() == 2 {
                    Some(SourceInfo {
                        package: parts[0].to_string(),
                        function: parts[1].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();
        result.sort_by(|a, b| {
            a.package.cmp(&b.package).then(a.function.cmp(&b.function))
        });
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_content_hash_deterministic() {
        let data = b"hello world";
        let h1 = content_hash(data);
        let h2 = content_hash(data);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[tokio::test]
    async fn test_content_hash_different_data() {
        let h1 = content_hash(b"hello");
        let h2 = content_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn test_validate_id_valid() {
        let valid_id = "a".repeat(64);
        assert!(validate_id(&valid_id).is_ok());
    }

    #[tokio::test]
    async fn test_validate_id_too_short() {
        assert!(validate_id("abc123").is_err());
    }

    #[tokio::test]
    async fn test_validate_id_non_hex() {
        let invalid = format!("{}g", "a".repeat(63));
        assert!(validate_id(&invalid).is_err());
    }

    // --- Memory store tests ---

    #[tokio::test]
    async fn test_memory_store_and_get() {
        let store = MemoryArtifactStore::new();
        let data = b"wasm module bytes";
        let id = store.store(data).await.unwrap();
        assert_eq!(id.len(), 64);

        let retrieved = store.get(&id).await.unwrap();
        assert_eq!(retrieved, data);
    }

    #[tokio::test]
    async fn test_memory_store_dedup() {
        let store = MemoryArtifactStore::new();
        let data = b"same data";
        let id1 = store.store(data).await.unwrap();
        let id2 = store.store(data).await.unwrap();
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn test_memory_get_nonexistent() {
        let store = MemoryArtifactStore::new();
        let id = "a".repeat(64);
        let err = store.get(&id).await.unwrap_err();
        assert!(matches!(err, ArtifactError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_memory_delete() {
        let store = MemoryArtifactStore::new();
        let data = b"to delete";
        let id = store.store(data).await.unwrap();
        assert!(store.exists(&id).await.unwrap());
        assert!(store.delete(&id).await.unwrap());
        assert!(!store.exists(&id).await.unwrap());
        assert!(!store.delete(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_exists() {
        let store = MemoryArtifactStore::new();
        let id = "b".repeat(64);
        assert!(!store.exists(&id).await.unwrap());

        let data = b"exists test";
        let id = store.store(data).await.unwrap();
        assert!(store.exists(&id).await.unwrap());
    }

    // --- Memory source store tests ---

    #[tokio::test]
    async fn test_memory_source_store_and_get() {
        let store = MemoryArtifactStore::new();
        store
            .store_source("my-pkg", "my-fn", "const x = 1;")
            .await
            .unwrap();
        let src = store.get_source("my-pkg", "my-fn").await.unwrap();
        assert_eq!(src.as_deref(), Some("const x = 1;"));
    }

    #[tokio::test]
    async fn test_memory_source_get_nonexistent() {
        let store = MemoryArtifactStore::new();
        let src = store.get_source("nope", "nope").await.unwrap();
        assert!(src.is_none());
    }

    #[tokio::test]
    async fn test_memory_source_overwrite() {
        let store = MemoryArtifactStore::new();
        store.store_source("pkg", "fn1", "v1").await.unwrap();
        store.store_source("pkg", "fn1", "v2").await.unwrap();
        let src = store.get_source("pkg", "fn1").await.unwrap();
        assert_eq!(src.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn test_memory_source_delete() {
        let store = MemoryArtifactStore::new();
        store.store_source("pkg", "fn1", "code").await.unwrap();
        assert!(store.delete_source("pkg", "fn1").await.unwrap());
        assert!(!store.delete_source("pkg", "fn1").await.unwrap());
        assert!(store.get_source("pkg", "fn1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_memory_delete_package_sources() {
        let store = MemoryArtifactStore::new();
        store.store_source("pkg", "fn1", "a").await.unwrap();
        store.store_source("pkg", "fn2", "b").await.unwrap();
        store.store_source("other", "fn3", "c").await.unwrap();

        let count = store.delete_package_sources("pkg").await.unwrap();
        assert_eq!(count, 2);
        assert!(store.get_source("pkg", "fn1").await.unwrap().is_none());
        assert!(store.get_source("pkg", "fn2").await.unwrap().is_none());
        // Other package untouched
        assert!(store.get_source("other", "fn3").await.unwrap().is_some());
    }

    // --- Filesystem store tests ---

    #[tokio::test]
    async fn test_fs_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        let data = b"wasm module content";
        let id = store.store(data).await.unwrap();
        assert_eq!(id.len(), 64);

        let retrieved = store.get(&id).await.unwrap();
        assert_eq!(retrieved, data);

        assert!(store.exists(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_fs_store_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        let data = b"dedup data";
        let id1 = store.store(data).await.unwrap();
        let id2 = store.store(data).await.unwrap();
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn test_fs_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        let data = b"to be deleted";
        let id = store.store(data).await.unwrap();
        assert!(store.delete(&id).await.unwrap());
        assert!(!store.exists(&id).await.unwrap());
        assert!(!store.delete(&id).await.unwrap());
    }

    #[tokio::test]
    async fn test_fs_get_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        let id = "c".repeat(64);
        let err = store.get(&id).await.unwrap_err();
        assert!(matches!(err, ArtifactError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_fs_source_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        let source = r#"
import { service, method, OaaSObject } from "@oaas/sdk";

@service("Counter")
class Counter extends OaaSObject {
    count: number = 0;

    @method()
    async increment(): Promise<number> {
        this.count += 1;
        return this.count;
    }
}
export default Counter;
"#;

        store
            .store_source("example", "counter", source)
            .await
            .unwrap();
        let retrieved = store.get_source("example", "counter").await.unwrap();
        assert_eq!(retrieved.as_deref(), Some(source));
    }

    #[tokio::test]
    async fn test_fs_source_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        store.store_source("pkg", "fn1", "v1").await.unwrap();
        store.store_source("pkg", "fn1", "v2").await.unwrap();
        let src = store.get_source("pkg", "fn1").await.unwrap();
        assert_eq!(src.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn test_fs_source_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        store.store_source("pkg", "fn1", "code").await.unwrap();
        assert!(store.delete_source("pkg", "fn1").await.unwrap());
        assert!(!store.delete_source("pkg", "fn1").await.unwrap());
        assert!(store.get_source("pkg", "fn1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_fs_delete_package_sources() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsArtifactStore::new(dir.path()).await.unwrap();

        store.store_source("pkg", "a", "1").await.unwrap();
        store.store_source("pkg", "b", "2").await.unwrap();
        store.store_source("other", "c", "3").await.unwrap();

        let count = store.delete_package_sources("pkg").await.unwrap();
        assert_eq!(count, 2);
        assert!(store.get_source("pkg", "a").await.unwrap().is_none());
        assert!(store.get_source("other", "c").await.unwrap().is_some());
    }
}
