//! WASM module fetching, compilation, and caching.

use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};
use wasmtime::Engine;
use wasmtime::component::Component;

/// Which WIT world a compiled WASM component targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldType {
    /// Procedural world: guest exports `guest-function` (invoke-fn / invoke-obj).
    Legacy,
    /// OOP world: guest exports `guest-object` (on-invoke with object-proxy).
    ObjectOriented,
}

/// Compiled WASM component with metadata
pub struct CompiledModule {
    pub component: Component,
    pub source_url: String,
    pub world_type: WorldType,
}

/// Detect the world type by inspecting the component's exported interface names.
fn detect_world_type(engine: &Engine, component: &Component) -> WorldType {
    let ct = component.component_type();
    for (name, _) in ct.exports(engine) {
        if name.contains("guest-object") {
            return WorldType::ObjectOriented;
        }
    }
    WorldType::Legacy
}

/// Fetches, compiles, and caches WASM components by function ID.
pub struct WasmModuleStore {
    engine: Engine,
    modules: Arc<RwLock<HashMap<String, Arc<CompiledModule>>>>,
}

impl WasmModuleStore {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            modules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load a WASM component from a URL (HTTP/HTTPS/file://), compile, and cache it.
    pub async fn load(
        &self,
        fn_id: &str,
        url: &str,
    ) -> Result<Arc<CompiledModule>> {
        info!(fn_id, url, "loading WASM module");
        let bytes = fetch_module(url).await?;
        let component = Component::from_binary(&self.engine, &bytes)?;
        let world_type = detect_world_type(&self.engine, &component);
        let module = Arc::new(CompiledModule {
            component,
            source_url: url.to_string(),
            world_type,
        });
        self.modules
            .write()
            .await
            .insert(fn_id.to_string(), module.clone());
        info!(fn_id, url, ?world_type, "WASM module loaded and cached");
        Ok(module)
    }

    /// Load a WASM component directly from bytes (useful for testing).
    pub async fn load_from_bytes(
        &self,
        fn_id: &str,
        bytes: &[u8],
    ) -> Result<Arc<CompiledModule>> {
        debug!(fn_id, len = bytes.len(), "loading WASM module from bytes");
        let component = Component::from_binary(&self.engine, bytes)?;
        let world_type = detect_world_type(&self.engine, &component);
        let module = Arc::new(CompiledModule {
            component,
            source_url: "bytes://".to_string(),
            world_type,
        });
        self.modules
            .write()
            .await
            .insert(fn_id.to_string(), module.clone());
        Ok(module)
    }

    /// Get a previously loaded module by function ID.
    pub async fn get(&self, fn_id: &str) -> Option<Arc<CompiledModule>> {
        self.modules.read().await.get(fn_id).cloned()
    }

    /// Insert a pre-compiled module under a function ID (for deduplication).
    pub async fn insert(&self, fn_id: &str, module: Arc<CompiledModule>) {
        self.modules
            .write()
            .await
            .insert(fn_id.to_string(), module);
    }

    /// Remove a cached module.
    pub async fn remove(&self, fn_id: &str) -> bool {
        let removed = self.modules.write().await.remove(fn_id).is_some();
        if removed {
            debug!(fn_id, "removed WASM module from cache");
        }
        removed
    }

    /// Number of cached modules.
    pub async fn len(&self) -> usize {
        self.modules.read().await.len()
    }

    /// Whether the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.modules.read().await.is_empty()
    }

    /// Reference to the wasmtime engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// Fetch module bytes from a URL
async fn fetch_module(url: &str) -> Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        debug!(path, "reading WASM module from filesystem");
        tokio::fs::read(path).await.with_context(|| {
            format!("failed to read WASM module from {}", path)
        })
    } else if url.starts_with("http://") || url.starts_with("https://") {
        debug!(url, "fetching WASM module over HTTP");
        let resp = reqwest::get(url).await.with_context(|| {
            format!("failed to fetch WASM module from {}", url)
        })?;
        if !resp.status().is_success() {
            bail!("HTTP {} fetching WASM module from {}", resp.status(), url);
        }
        let bytes = resp.bytes().await.with_context(|| {
            format!("failed to read response body from {}", url)
        })?;
        Ok(bytes.to_vec())
    } else {
        bail!("unsupported WASM module URL scheme: {}", url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> Engine {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        Engine::new(&config).unwrap()
    }

    // Minimal valid WASM component (empty component)
    fn minimal_component_bytes() -> Vec<u8> {
        // A minimal valid wasm component (component section magic + layer)
        // Using wat to produce a minimal component
        wat::parse_str("(component)").expect("valid minimal component WAT")
    }

    #[tokio::test]
    async fn load_from_bytes_and_get() {
        let store = WasmModuleStore::new(test_engine());
        let bytes = minimal_component_bytes();
        let module = store.load_from_bytes("test-fn", &bytes).await.unwrap();
        assert_eq!(module.source_url, "bytes://");

        let cached = store.get("test-fn").await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let store = WasmModuleStore::new(test_engine());
        assert!(store.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn remove_drops_module() {
        let store = WasmModuleStore::new(test_engine());
        let bytes = minimal_component_bytes();
        store.load_from_bytes("fn1", &bytes).await.unwrap();
        assert_eq!(store.len().await, 1);

        assert!(store.remove("fn1").await);
        assert_eq!(store.len().await, 0);
        assert!(store.get("fn1").await.is_none());
    }

    #[tokio::test]
    async fn remove_nonexistent_returns_false() {
        let store = WasmModuleStore::new(test_engine());
        assert!(!store.remove("nope").await);
    }

    #[tokio::test]
    async fn load_replaces_existing() {
        let store = WasmModuleStore::new(test_engine());
        let bytes = minimal_component_bytes();
        store.load_from_bytes("fn1", &bytes).await.unwrap();
        store.load_from_bytes("fn1", &bytes).await.unwrap();
        assert_eq!(store.len().await, 1);
    }

    #[tokio::test]
    async fn fetch_module_unsupported_scheme() {
        let result = fetch_module("ftp://example.com/fn.wasm").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported"));
    }

    // ─── WorldType detection ────────────────────────────────

    #[tokio::test]
    async fn minimal_component_is_legacy() {
        let store = WasmModuleStore::new(test_engine());
        let bytes = minimal_component_bytes();
        let module = store.load_from_bytes("empty", &bytes).await.unwrap();
        // A bare component with no exports should be detected as Legacy
        assert_eq!(module.world_type, WorldType::Legacy);
    }

    #[test]
    fn detect_world_type_empty_component() {
        let engine = test_engine();
        let bytes = minimal_component_bytes();
        let component = Component::from_binary(&engine, &bytes).unwrap();
        assert_eq!(detect_world_type(&engine, &component), WorldType::Legacy);
    }
}
