//! Data structures and message types for MST replication

use merkle_search_tree::{
    diff::DiffRange, diff::PageRange, digest::PageDigest,
};
use oprc_dp_storage::StorageResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Key wrapper for MST operations
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct MstKey(pub u64);

impl From<u64> for MstKey {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<MstKey> for u64 {
    fn from(value: MstKey) -> Self {
        value.0
    }
}

impl AsRef<[u8]> for MstKey {
    fn as_ref(&self) -> &[u8] {
        // For MST, we need a byte representation of the key
        // We'll use a static buffer approach
        unsafe {
            std::slice::from_raw_parts(&self.0 as *const u64 as *const u8, 8)
        }
    }
}

/// Configuration for merge functions and timestamp extraction
pub struct MstConfig<T> {
    /// Extract timestamp from a value for LWW comparison
    pub extract_timestamp: Box<dyn Fn(&T) -> u64 + Send + Sync>,
    /// Merge two values (local, remote) -> result
    pub merge_function: Box<dyn Fn(T, T, u64) -> T + Send + Sync>, // (local, remote, node_id) -> merged
    /// Serialize value to bytes
    pub serialize: Box<dyn Fn(&T) -> StorageResult<Vec<u8>> + Send + Sync>,
    /// Deserialize bytes to value
    pub deserialize: Box<dyn Fn(&[u8]) -> StorageResult<T> + Send + Sync>,
}

/// Generic message types for MST synchronization
#[derive(Serialize, Deserialize)]
pub struct GenericPageQuery {
    pub start_bounds: u64,
    pub end_bounds: u64,
}

#[derive(Serialize, Deserialize)]
pub struct GenericLoadPageReq {
    pub pages: Vec<GenericPageQuery>,
}

impl GenericLoadPageReq {
    pub fn from_diff(diffs: Vec<DiffRange<MstKey>>) -> Self {
        let pages = diffs
            .iter()
            .map(|p| GenericPageQuery {
                start_bounds: p.start().0,
                end_bounds: p.end().0,
            })
            .collect();
        Self { pages }
    }
}

#[derive(Serialize, Deserialize)]
pub struct GenericPagesResp<T> {
    pub items: BTreeMap<u64, T>,
}

#[derive(Serialize, Deserialize)]
pub struct GenericPageRangeMessage {
    pub owner: u64,
    pub pages: Vec<GenericNetworkPage>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct GenericNetworkPage {
    start_bounds: MstKey,
    end_bounds: MstKey,
    hash: [u8; 16],
}

impl GenericNetworkPage {
    pub fn to_page_range(
        list: &[GenericNetworkPage],
    ) -> Vec<PageRange<'_, MstKey>> {
        list.iter()
            .map(|p| {
                PageRange::new(
                    &p.start_bounds,
                    &p.end_bounds,
                    PageDigest::new(p.hash),
                )
            })
            .collect()
    }

    pub fn from_page_range(page: &PageRange<MstKey>) -> Self {
        Self {
            start_bounds: page.start().to_owned(),
            end_bounds: page.end().to_owned(),
            hash: *page.hash().as_bytes(),
        }
    }

    pub fn from_page_ranges(pages: Vec<PageRange<MstKey>>) -> Vec<Self> {
        pages
            .iter()
            .map(|page| Self::from_page_range(page))
            .collect()
    }
}
