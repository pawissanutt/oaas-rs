//! Environment variable test helpers.
//!
//! Provides ergonomic, test-friendly ways to set env vars (with unsafe suppression to
//! match existing patterns) plus a guard that restores previous values on drop.

/// RAII guard that restores (or unsets) the original value when dropped.
pub struct EnvGuard {
    key: String,
    prev: Option<String>,
}

impl EnvGuard {
    pub fn key(&self) -> &str {
        &self.key
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(ref v) = self.prev {
                std::env::set_var(&self.key, v);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }
}

/// Set an environment variable (unsafe block to mirror existing tests) without guard.
pub fn set_env(key: &str, val: &str) {
    unsafe { std::env::set_var(key, val) }
}

/// Set an environment variable returning a guard that restores the previous value when dropped.
pub fn set_env_guarded(key: &str, val: &str) -> EnvGuard {
    let prev = std::env::var(key).ok();
    unsafe {
        std::env::set_var(key, val);
    }
    EnvGuard {
        key: key.to_string(),
        prev,
    }
}

/// Apply multiple env vars, returning guards in same order.
pub fn set_envs_guarded<'a, I>(kvs: I) -> Vec<EnvGuard>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    kvs.into_iter()
        .map(|(k, v)| set_env_guarded(k, v))
        .collect()
}

/// Builder-style collection of environment guards. Dropping restores all keys.
pub struct Env {
    guards: Vec<EnvGuard>,
}

impl Env {
    /// Create empty builder.
    pub fn new() -> Self {
        Self { guards: Vec::new() }
    }

    /// Preallocate with capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            guards: Vec::with_capacity(cap),
        }
    }

    /// Set a key -> val, capturing previous value; chainable.
    pub fn set(mut self, key: &str, val: &str) -> Self {
        self.guards.push(set_env_guarded(key, val));
        self
    }

    /// Mutable variant for iterative construction.
    pub fn insert(&mut self, key: &str, val: &str) {
        self.guards.push(set_env_guarded(key, val));
    }

    /// Bulk set from iterator.
    pub fn extend<'a, I>(mut self, kvs: I) -> Self
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        for (k, v) in kvs {
            self.guards.push(set_env_guarded(k, v));
        }
        self
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
