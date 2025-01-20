use std::collections::BTreeMap;

use merkle_search_tree::{
    diff::{DiffRange, PageRange},
    digest::PageDigest,
};

use crate::shard::ObjectEntry;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PageQuery {
    pub start_bounds: u64,
    pub end_bounds: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LoadPageReq {
    pub pages: Vec<PageQuery>,
}

impl LoadPageReq {
    pub fn from_diff(diffs: Vec<DiffRange<Key>>) -> Self {
        let pages = diffs
            .iter()
            .map(|p| PageQuery {
                start_bounds: p.start().0,
                end_bounds: p.end().0,
            })
            .collect();
        Self { pages }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PagesResp {
    pub items: BTreeMap<u64, ObjectEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PageRangeMessage {
    pub owner: u64,
    pub pages: Vec<NetworkPage>,
}

#[derive(
    Clone,
    Debug,
    PartialEq,
    PartialOrd,
    Eq,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Key(pub u64);

impl AsRef<[u8]> for Key {
    fn as_ref(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Key).cast::<u8>(),
                std::mem::size_of::<u64>(),
            )
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct NetworkPage {
    start_bounds: Key,
    end_bounds: Key,
    hash: [u8; 16],
}

impl NetworkPage {
    #[inline]
    pub fn to_page_range<'a>(list: &Vec<NetworkPage>) -> Vec<PageRange<Key>> {
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

    #[inline]
    pub fn from_page_range(page: &PageRange<Key>) -> Self {
        Self {
            start_bounds: page.start().to_owned(),
            end_bounds: page.end().to_owned(),
            hash: *page.hash().as_bytes(),
        }
    }
    #[inline]
    pub fn from_page_ranges(pages: Vec<PageRange<Key>>) -> Vec<Self> {
        pages
            .iter()
            .map(|page| Self::from_page_range(page))
            .collect()
    }
}
