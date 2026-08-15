//! Turning a blob of bytes into an index that can be searched without checks.
//!
//! Everything is validated once, here. [`IndexView::parse`] either returns a
//! view whose every range is known to be inside the buffer, or it returns the
//! reason it refused — so the search in [`crate::search`] indexes freely and
//! the module cannot be made to trap by a malformed file.
//!
//! Refusing is safe: `init` reports 0 and the page falls back to its JavaScript
//! search engine.

use crate::format::{
    u16_le, u32_le, DIR_STRIDE, GROUP_STRIDE, HEADER, ITEM_STRIDE, MAGIC, MAX_POOL,
};
use std::ops::Range;

/// A CTH group: its display label and the items belonging to it.
pub struct Group {
    /// Pre-rendered `cth 786` label — built once, matched on every query.
    pub label: String,
    pub items: Range<usize>,
}

/// One manuscript: where its searchable text lives, plus pooled metadata ids.
pub struct Item {
    pub sig: Range<usize>,
    pub auth: u8,
    pub year: u8,
}

/// A fully validated index. Every range here was bounds-checked in [`parse`],
/// so lookups below cannot go out of bounds.
///
/// [`parse`]: IndexView::parse
pub struct IndexView {
    /// Owns the pooled strings that the ranges point into.
    bytes: Vec<u8>,
    pub groups: Vec<Group>,
    pub items: Vec<Item>,
    pub auth: Vec<Range<usize>>,
    pub year: Vec<Range<usize>>,
}

/// Why a blob was rejected. The caller only sees `0`, but naming the cases keeps
/// the validation readable and lets the tests be specific.
#[derive(Debug, PartialEq, Eq)]
pub enum IndexError {
    TooShort,
    BadMagic,
    PoolTooLarge,
    Truncated,
    /// An offset/length pair points outside the pool it belongs to.
    RangeOutOfPool,
    /// A group claims items that do not exist.
    GroupOutOfRange,
    /// An item points at an author or year that is not in the pool.
    MetaIdOutOfRange,
}

impl IndexView {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, IndexError> {
        if bytes.len() < HEADER {
            return Err(IndexError::TooShort);
        }
        if u32_le(&bytes, 0) != MAGIC {
            return Err(IndexError::BadMagic);
        }
        let n_groups = u32_le(&bytes, 4) as usize;
        let n_items = u32_le(&bytes, 8) as usize;
        let n_auth = u32_le(&bytes, 12);
        let n_year = u32_le(&bytes, 16);
        let sig_pool_len = u32_le(&bytes, 20) as usize;
        let auth_pool_len = u32_le(&bytes, 24) as usize;
        let year_pool_len = u32_le(&bytes, 28) as usize;

        if n_auth > MAX_POOL || n_year > MAX_POOL {
            return Err(IndexError::PoolTooLarge);
        }

        // Section offsets, each starting where the previous one ends.
        let groups_off = HEADER;
        let items_off = groups_off + n_groups * GROUP_STRIDE;
        let auth_dir_off = items_off + n_items * ITEM_STRIDE;
        let year_dir_off = auth_dir_off + n_auth as usize * DIR_STRIDE;
        let sig_pool_off = year_dir_off + n_year as usize * DIR_STRIDE;
        let auth_pool_off = sig_pool_off + sig_pool_len;
        let year_pool_off = auth_pool_off + auth_pool_len;
        if bytes.len() < year_pool_off + year_pool_len {
            return Err(IndexError::Truncated);
        }

        let items = (0..n_items)
            .map(|i| {
                let at = items_off + i * ITEM_STRIDE;
                let off = u32_le(&bytes, at) as usize;
                let len = bytes[at + 4] as usize;
                let auth = bytes[at + 5];
                let year = bytes[at + 6];
                // Ids index the `u64` bitsets built in `matching`, so an id past
                // its pool is not merely meaningless — it would shift a `u64` by
                // up to 255. That is an overflow: a build with overflow checks
                // panics, and a panic crossing `extern "C"` takes the whole
                // module down; without them the shift is masked and the item
                // silently matches the wrong pool entry. Reject the blob here
                // and the caller falls back to the JavaScript engine.
                if u32::from(auth) >= n_auth || u32::from(year) >= n_year {
                    return Err(IndexError::MetaIdOutOfRange);
                }
                Ok(Item {
                    sig: pool_range(sig_pool_off, sig_pool_len, off, len)?,
                    auth,
                    year,
                })
            })
            .collect::<Result<Vec<_>, IndexError>>()?;

        let groups = (0..n_groups)
            .map(|g| {
                let at = groups_off + g * GROUP_STRIDE;
                let cth = u16_le(&bytes, at);
                let count = u16_le(&bytes, at + 2) as usize;
                let start = u32_le(&bytes, at + 4) as usize;
                let end = start.checked_add(count).ok_or(IndexError::GroupOutOfRange)?;
                if end > n_items {
                    return Err(IndexError::GroupOutOfRange);
                }
                Ok(Group {
                    label: format!("cth {cth}"),
                    items: start..end,
                })
            })
            .collect::<Result<Vec<_>, IndexError>>()?;

        let auth = directory(&bytes, auth_dir_off, n_auth, auth_pool_off, auth_pool_len)?;
        let year = directory(&bytes, year_dir_off, n_year, year_pool_off, year_pool_len)?;

        Ok(Self {
            bytes,
            groups,
            items,
            auth,
            year,
        })
    }

    /// The searchable text of one item.
    pub fn sig(&self, item: &Item) -> &[u8] {
        &self.bytes[item.sig.clone()]
    }

    /// The text of one pool entry.
    pub fn pooled(&self, range: &Range<usize>) -> &[u8] {
        &self.bytes[range.clone()]
    }
}

/// Read a directory of `(offset, length)` pairs, checking each against its pool.
fn directory(
    bytes: &[u8],
    dir_off: usize,
    count: u32,
    pool_off: usize,
    pool_len: usize,
) -> Result<Vec<Range<usize>>, IndexError> {
    (0..count as usize)
        .map(|i| {
            let at = dir_off + i * DIR_STRIDE;
            let off = u16_le(bytes, at) as usize;
            let len = u16_le(bytes, at + 2) as usize;
            pool_range(pool_off, pool_len, off, len)
        })
        .collect()
}

/// Turn a pool-relative `(offset, length)` into an absolute range, rejecting
/// anything that would reach past the pool.
fn pool_range(
    pool_off: usize,
    pool_len: usize,
    off: usize,
    len: usize,
) -> Result<Range<usize>, IndexError> {
    let end = off.checked_add(len).ok_or(IndexError::RangeOutOfPool)?;
    if end > pool_len {
        return Err(IndexError::RangeOutOfPool);
    }
    Ok(pool_off + off..pool_off + end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::build_tiny;

    #[track_caller]
    fn assert_rejected(blob: Vec<u8>, expected: IndexError) {
        match IndexView::parse(blob) {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("expected {expected:?}, but the blob was accepted"),
        }
    }

    #[test]
    fn malformed_blobs_are_rejected_by_reason() {
        assert_rejected(vec![0; 4], IndexError::TooShort);
        assert_rejected(vec![0; HEADER], IndexError::BadMagic);

        let mut truncated = build_tiny();
        truncated.truncate(truncated.len() - 1);
        assert_rejected(truncated, IndexError::Truncated);

        let mut too_many_auth = build_tiny();
        too_many_auth[12..16].copy_from_slice(&(MAX_POOL + 1).to_le_bytes());
        assert_rejected(too_many_auth, IndexError::PoolTooLarge);

        let mut bad_group = build_tiny();
        // Group 0 claims 9 items; only 2 exist.
        bad_group[HEADER + 2..HEADER + 4].copy_from_slice(&9u16.to_le_bytes());
        assert_rejected(bad_group, IndexError::GroupOutOfRange);
    }

    /// An id past its pool must be refused at load, not shifted at search time.
    ///
    /// Before this was checked, `init` accepted the blob and the first query
    /// shifted a `u64` by 200: with overflow checks that is a panic, and with
    /// `panic = "abort"` a panic here ends the module. Without them the shift is
    /// masked to `200 & 63`, so the item quietly matches pool entry 8.
    #[test]
    fn an_id_past_its_pool_is_rejected() {
        let items_off = HEADER + GROUP_STRIDE;
        for offset in [5 /* auth */, 6 /* year */] {
            for bad in [1u8, 64, 200, 255] {
                let mut blob = build_tiny(); // one auth entry, one year entry
                blob[items_off + offset] = bad;
                assert_rejected(blob, IndexError::MetaIdOutOfRange);
            }
        }
    }

    /// Regression: an item offset pointing outside the siglum pool must be
    /// refused rather than read.
    #[test]
    fn an_item_offset_past_the_siglum_pool_is_rejected() {
        let mut blob = build_tiny();
        // First item's `sig_off` lives right after the header and group table.
        let sig_off_at = HEADER + GROUP_STRIDE;
        blob[sig_off_at..sig_off_at + 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        assert_rejected(blob, IndexError::RangeOutOfPool);
    }
}
