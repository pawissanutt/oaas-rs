#![cfg(feature = "redb")]

use std::ops::Bound;

use crate::StorageError;
use redb::TableDefinition;

// Single KV table for this backend
pub const KV: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("kv");

pub fn map_redb_err(e: redb::Error) -> StorageError {
    StorageError::backend(e.to_string())
}

pub fn exclusive_upper_bound(prefix: &[u8]) -> (Option<Vec<u8>>, Bound<&[u8]>) {
    if prefix.is_empty() {
        return (None, Bound::Unbounded);
    }
    let mut end = prefix.to_vec();
    if let Some(last) = end.last_mut() {
        if *last == u8::MAX {
            end.push(0);
        } else {
            *last += 1;
        }
    }
    let end_ref: Bound<&[u8]> = Bound::Excluded(end.as_slice());
    (Some(end), end_ref)
}

pub fn convert_range_bounds<R>(range: &R) -> (
    Bound<&[u8]>,
    Bound<&[u8]>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
) where
    R: std::ops::RangeBounds<Vec<u8>>,
{
    use std::ops::RangeBounds;

    let start_buf: Option<Vec<u8>> = match range.start_bound() {
        std::ops::Bound::Included(v) => Some(v.clone()),
        std::ops::Bound::Excluded(v) => Some(v.clone()),
        std::ops::Bound::Unbounded => None,
    };
    let end_buf: Option<Vec<u8>> = match range.end_bound() {
        std::ops::Bound::Included(v) => Some(v.clone()),
        std::ops::Bound::Excluded(v) => Some(v.clone()),
        std::ops::Bound::Unbounded => None,
    };

    let start_bound: Bound<&[u8]> = match (range.start_bound(), &start_buf) {
        (std::ops::Bound::Included(_), Some(b)) => Bound::Included(b.as_slice()),
        (std::ops::Bound::Excluded(_), Some(b)) => Bound::Excluded(b.as_slice()),
        _ => Bound::Unbounded,
    };
    let end_bound: Bound<&[u8]> = match (range.end_bound(), &end_buf) {
        (std::ops::Bound::Included(_), Some(b)) => Bound::Included(b.as_slice()),
        (std::ops::Bound::Excluded(_), Some(b)) => Bound::Excluded(b.as_slice()),
        _ => Bound::Unbounded,
    };

    (start_bound, end_bound, start_buf, end_buf)
}
