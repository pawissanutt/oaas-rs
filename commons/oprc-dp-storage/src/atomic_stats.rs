use crate::{StorageBackendType, StorageStats};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Thread-safe atomic statistics tracker that avoids expensive O(n) operations
pub struct AtomicStats {
    entries_count: AtomicU64,
    total_size_bytes: AtomicU64,
    backend_type: StorageBackendType,
}

impl AtomicStats {
    /// Create new atomic stats tracker
    pub fn new(backend_type: StorageBackendType) -> Self {
        Self {
            entries_count: AtomicU64::new(0),
            total_size_bytes: AtomicU64::new(0),
            backend_type,
        }
    }

    /// Record a new entry being added
    #[inline]
    pub fn record_put(
        &self,
        key_size: usize,
        value_size: usize,
        was_update: bool,
    ) {
        let total_size = (key_size + value_size) as u64;

        if !was_update {
            self.entries_count.fetch_add(1, Ordering::Relaxed);
        }
        self.total_size_bytes
            .fetch_add(total_size, Ordering::Relaxed);
    }

    /// Record an entry being updated (old entry replaced)
    #[inline]
    pub fn record_put_with_old_size(
        &self,
        key_size: usize,
        new_value_size: usize,
        old_value_size: Option<usize>,
    ) {
        let new_total = (key_size + new_value_size) as u64;

        match old_value_size {
            Some(old_size) => {
                // Update: replace old size with new size
                let old_total = (key_size + old_size) as u64;
                if new_total >= old_total {
                    self.total_size_bytes
                        .fetch_add(new_total - old_total, Ordering::Relaxed);
                } else {
                    self.total_size_bytes
                        .fetch_sub(old_total - new_total, Ordering::Relaxed);
                }
            }
            None => {
                // New entry
                self.entries_count.fetch_add(1, Ordering::Relaxed);
                self.total_size_bytes
                    .fetch_add(new_total, Ordering::Relaxed);
            }
        }
    }

    /// Record an entry being deleted
    #[inline]
    pub fn record_delete(&self, key_size: usize, value_size: usize) {
        let total_size = (key_size + value_size) as u64;

        self.entries_count.fetch_sub(1, Ordering::Relaxed);
        self.total_size_bytes
            .fetch_sub(total_size, Ordering::Relaxed);
    }

    /// Record multiple entries being deleted (for delete_range)
    #[inline]
    pub fn record_delete_batch(&self, total_entries: u64, total_size: u64) {
        self.entries_count
            .fetch_sub(total_entries, Ordering::Relaxed);
        self.total_size_bytes
            .fetch_sub(total_size, Ordering::Relaxed);
    }

    /// Reset all counters (for clear operations)
    #[inline]
    pub fn reset(&self) {
        self.entries_count.store(0, Ordering::Relaxed);
        self.total_size_bytes.store(0, Ordering::Relaxed);
    }

    /// Set counters to specific values (for restore operations)
    #[inline]
    pub fn set_counts(&self, entries: u64, total_size: u64) {
        self.entries_count.store(entries, Ordering::Relaxed);
        self.total_size_bytes.store(total_size, Ordering::Relaxed);
    }

    /// Get current entry count
    #[inline]
    pub fn entries_count(&self) -> u64 {
        self.entries_count.load(Ordering::Relaxed)
    }

    /// Get current total size
    #[inline]
    pub fn total_size_bytes(&self) -> u64 {
        self.total_size_bytes.load(Ordering::Relaxed)
    }

    /// Build a complete StorageStats snapshot
    pub fn to_storage_stats(&self) -> StorageStats {
        let entries = self.entries_count.load(Ordering::Relaxed);
        let total_size = self.total_size_bytes.load(Ordering::Relaxed);

        StorageStats {
            entries_count: entries,
            total_size_bytes: total_size,
            memory_usage_bytes: match self.backend_type {
                StorageBackendType::Memory => Some(total_size),
                _ => Some(total_size), // SkipList is also in-memory
            },
            disk_usage_bytes: match self.backend_type {
                StorageBackendType::Memory => Some(0),
                _ => Some(0), // SkipList doesn't use disk
            },
            cache_hit_rate: Some(1.0), // In-memory backends always hit
            backend_specific: HashMap::new(),
        }
    }
}

impl Clone for AtomicStats {
    fn clone(&self) -> Self {
        Self {
            entries_count: AtomicU64::new(
                self.entries_count.load(Ordering::Relaxed),
            ),
            total_size_bytes: AtomicU64::new(
                self.total_size_bytes.load(Ordering::Relaxed),
            ),
            backend_type: self.backend_type.clone(),
        }
    }
}
