//! Compact inventory search over the TLH2 index.
//!
//! The module is loaded by `src/lib/wasm-search.ts` and answers one question:
//! which CTH groups and which manuscripts inside them contain the query string.
//! Queries and index are both lowercase ASCII-folded on the JavaScript side.
//!
//! # Performance strategy
//!
//! Aruna is not hot-path software. This module runs once per keystroke over
//! ~25 000 short strings, so the budget is a single frame. Plain substring
//! matching meets it; there is deliberately no hand-rolled automaton, no skip
//! table and no bloom prefilter here. Anything reintroduced must come with
//! before/after keystroke latency measured in the browser (Performance profile
//! of the tab) — see `PERFORMANCE.md`.
//!
//! `unsafe` appears only in the five `extern "C"` entry points (`alloc`,
//! `dealloc`, `init`, `reset`, `search`), where it is unavoidable — everything
//! below the boundary is safe Rust.
//!
//! # Index layout (TLH2, little-endian)
//!
//! ```text
//! header 32 B | groups 8 B each | items 8 B each | auth_dir 4 B | year_dir 4 B | pools
//! ```
//!
//! Header: magic, n_groups, n_items, n_auth, n_year, sig_pool_len,
//! auth_pool_len, year_pool_len — eight `u32`s.
//! Group: cth `u16`, item count `u16`, first item index `u32`.
//! Item: siglum offset `u32`, siglum length `u8`, auth id `u8`, year id `u8`, pad.
//! Directory entry: pool offset `u16`, length `u16`.

use std::alloc::{alloc as raw_alloc, dealloc as raw_dealloc, Layout};
use std::cell::RefCell;
use std::ops::Range;

const MAGIC: u32 = 0x3248_4C54; // TLH2
const HEADER: usize = 32;
const GROUP_STRIDE: usize = 8;
const ITEM_STRIDE: usize = 8;
const DIR_STRIDE: usize = 4;
const RESULT_STRIDE: usize = 12;

/// Auth and year ids are matched through `u64` bitsets, which caps both pools.
const MAX_POOL: u32 = 64;

thread_local! {
    /// wasm32 is single-threaded; a `RefCell` is enough and costs no lock.
    static INDEX: RefCell<Option<IndexView>> = const { RefCell::new(None) };
}

/// A CTH group: its display label and the items belonging to it.
struct Group {
    /// Pre-rendered `cth 786` label — built once, matched on every query.
    label: String,
    items: Range<usize>,
}

/// One manuscript: where its searchable text lives, plus pooled metadata ids.
struct Item {
    sig: Range<usize>,
    auth: u8,
    year: u8,
}

/// A fully validated index. Every range here was bounds-checked in [`parse`],
/// so lookups below cannot go out of bounds.
///
/// [`parse`]: IndexView::parse
struct IndexView {
    /// Owns the pooled strings that the ranges point into.
    bytes: Vec<u8>,
    groups: Vec<Group>,
    items: Vec<Item>,
    auth: Vec<Range<usize>>,
    year: Vec<Range<usize>>,
}

/// Why a blob was rejected. The caller only sees `0`, but naming the cases keeps
/// the validation readable and lets the tests be specific.
#[derive(Debug, PartialEq, Eq)]
enum IndexError {
    TooShort,
    BadMagic,
    PoolTooLarge,
    Truncated,
    /// An offset/length pair points outside the pool it belongs to.
    RangeOutOfPool,
    /// A group claims items that do not exist.
    GroupOutOfRange,
}

impl IndexView {
    fn parse(bytes: Vec<u8>) -> Result<Self, IndexError> {
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
                Ok(Item {
                    sig: pool_range(sig_pool_off, sig_pool_len, off, len)?,
                    auth: bytes[at + 5],
                    year: bytes[at + 6],
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

    fn sig(&self, item: &Item) -> &[u8] {
        &self.bytes[item.sig.clone()]
    }

    /// Bitset of pool entries containing `query`. Pools hold at most 64 entries,
    /// so this is one pass over the pool instead of one per item.
    fn matching(&self, pool: &[Range<usize>], query: &[u8]) -> u64 {
        let mut bits = 0u64;
        for (i, range) in pool.iter().enumerate() {
            if contains(&self.bytes[range.clone()], query) {
                bits |= 1u64 << i;
            }
        }
        bits
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

fn u32_le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn u16_le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// Substring test — the only matcher in this module.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|window| window == needle)
}

// ── exports ─────────────────────────────────────────────────────
//
// The JavaScript side owns the protocol: it allocates, copies bytes in, calls,
// reads the result buffer and frees. Every entry point is total — a bad
// argument yields 0, never a trap, because a panic crossing `extern "C"`
// aborts the whole module.

/// Allocate `n` bytes for the caller to write into.
///
/// Returns null when `n` is 0 or the allocation fails. The caller must release
/// it with [`dealloc`] passing the same `n`.
#[no_mangle]
pub extern "C" fn alloc(n: usize) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(n, 1) else {
        return core::ptr::null_mut();
    };
    if n == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: `layout` has a non-zero size, checked above.
    unsafe { raw_alloc(layout) }
}

/// Release a block obtained from [`alloc`].
///
/// # Safety
///
/// `ptr` must come from [`alloc`] and `n` must be the size it was allocated
/// with. Passing null or `n == 0` is a no-op.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, n: usize) {
    if ptr.is_null() || n == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(n, 1) {
        // SAFETY: the caller guarantees `ptr` came from `alloc(n)`, which used
        // this same `Layout::from_size_align(n, 1)`; null and `n == 0` returned
        // above, so the block is live and the layout matches the allocation.
        raw_dealloc(ptr, layout);
    }
}

/// Load a TLH2 index. Returns 1 on success, 0 if the blob is malformed.
///
/// Calling it again replaces the loaded index; the previous one is dropped.
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn init(ptr: *const u8, len: usize) -> u32 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `ptr`; null and
    // `len == 0` returned above. The slice is copied into an owned `Vec`
    // before this function returns, so the borrow never outlives the call and
    // the index keeps no pointer into JavaScript's memory.
    let bytes = core::slice::from_raw_parts(ptr, len).to_vec();
    match IndexView::parse(bytes) {
        // `try_borrow_mut` rather than `borrow_mut`: a re-entrant call from
        // JavaScript while a search holds the borrow would panic, and a panic
        // crossing `extern "C"` aborts the module — the page would lose search
        // altogether instead of seeing one call fail. Report 0 instead.
        Ok(view) => INDEX.with(|cell| match cell.try_borrow_mut() {
            Ok(mut slot) => {
                *slot = Some(view);
                1
            }
            Err(_) => 0,
        }),
        Err(_) => 0,
    }
}

/// Drop the loaded index.
///
/// A no-op while the index is busy — same reasoning as [`init`], except there is
/// no return value to carry the refusal. The caller can retry.
#[no_mangle]
pub extern "C" fn reset() {
    INDEX.with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            *slot = None;
        }
    });
}

/// Search the loaded index, writing `count` followed by `count` 12-byte entries
/// (group index, kind, item index within the group) into `out_ptr`.
///
/// An empty query returns every group. Returns the number of entries written,
/// or 0 when nothing matched or no index is loaded.
///
/// # Safety
///
/// `q_ptr` must point to `q_len` readable bytes (or be null when `q_len` is 0),
/// and `out_ptr` must point to `out_cap` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn search(
    q_ptr: *const u8,
    q_len: usize,
    out_ptr: *mut u8,
    out_cap: usize,
) -> u32 {
    if out_ptr.is_null() || out_cap < 4 {
        return 0;
    }
    // SAFETY: the caller guarantees `out_cap` writable bytes at `out_ptr`;
    // null and `out_cap < 4` returned above. wasm32 is single-threaded and the
    // module never retains `out`, so no other alias to this range exists while
    // the borrow lives.
    let out = core::slice::from_raw_parts_mut(out_ptr, out_cap);
    let query = if q_len == 0 || q_ptr.is_null() {
        &[][..]
    } else {
        // SAFETY: the caller guarantees `q_len` readable bytes at `q_ptr`, and
        // the null / zero-length case took the branch above. The query is only
        // read, and the borrow ends when this function returns.
        core::slice::from_raw_parts(q_ptr, q_len)
    };

    // Same reason as `init`: a borrow conflict must mean "no results", not an
    // aborted module.
    INDEX.with(|cell| match cell.try_borrow() {
        Ok(slot) => match slot.as_ref() {
            Some(index) => run_search(index, query, out),
            None => 0,
        },
        Err(_) => 0,
    })
}

fn run_search(index: &IndexView, query: &[u8], out: &mut [u8]) -> u32 {
    let max_entries = (out.len() - 4) / RESULT_STRIDE;
    let mut count = 0usize;

    if query.is_empty() {
        count = index.groups.len().min(max_entries);
        for gi in 0..count {
            write_entry(out, gi, gi as u32, 0, 0);
        }
        write_count(out, count);
        return count as u32;
    }

    // Metadata is pooled and deduplicated, so a query is compared against each
    // distinct author/year once rather than once per manuscript.
    let auth_bits = index.matching(&index.auth, query);
    let year_bits = index.matching(&index.year, query);

    'groups: for (gi, group) in index.groups.iter().enumerate() {
        if count >= max_entries {
            break;
        }
        // A group whose label matches stands for all of its manuscripts.
        if contains(group.label.as_bytes(), query) {
            write_entry(out, count, gi as u32, 0, 0);
            count += 1;
            continue;
        }

        for (offset, item) in index.items[group.items.clone()].iter().enumerate() {
            if count >= max_entries {
                break 'groups;
            }
            let meta_hit = auth_bits >> item.auth & 1 != 0 || year_bits >> item.year & 1 != 0;
            if meta_hit || contains(index.sig(item), query) {
                write_entry(out, count, gi as u32, 1, offset as u32);
                count += 1;
            }
        }
    }

    write_count(out, count);
    count as u32
}

fn write_count(out: &mut [u8], count: usize) {
    out[0..4].copy_from_slice(&(count as u32).to_le_bytes());
}

fn write_entry(out: &mut [u8], idx: usize, gi: u32, kind: u32, extra: u32) {
    let at = 4 + idx * RESULT_STRIDE;
    out[at..at + 4].copy_from_slice(&gi.to_le_bytes());
    out[at + 4..at + 8].copy_from_slice(&kind.to_le_bytes());
    out[at + 8..at + 12].copy_from_slice(&extra.to_le_bytes());
}

// ── tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Two manuscripts in one CTH group, one author and one year in the pools.
    fn build_tiny() -> Vec<u8> {
        let sigs = [b"kbo 3.22".as_slice(), b"kbo 22.5".as_slice()];
        let auth_pool = b"ls";
        let year_pool = b"2021";

        let mut sig_pool = Vec::new();
        let s0_off = 0u32;
        sig_pool.extend_from_slice(sigs[0]);
        let s1_off = sig_pool.len() as u32;
        sig_pool.extend_from_slice(sigs[1]);

        let mut buf = Vec::new();
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // groups
        buf.extend_from_slice(&2u32.to_le_bytes()); // items
        buf.extend_from_slice(&1u32.to_le_bytes()); // auth entries
        buf.extend_from_slice(&1u32.to_le_bytes()); // year entries
        buf.extend_from_slice(&(sig_pool.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(auth_pool.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(year_pool.len() as u32).to_le_bytes());
        // group 0: CTH 1, two items starting at 0
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        for (off, sig) in [(s0_off, sigs[0]), (s1_off, sigs[1])] {
            buf.extend_from_slice(&off.to_le_bytes());
            buf.push(sig.len() as u8);
            buf.push(0); // auth id
            buf.push(0); // year id
            buf.push(0); // pad
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(auth_pool.len() as u16).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&(year_pool.len() as u16).to_le_bytes());
        buf.extend_from_slice(&sig_pool);
        buf.extend_from_slice(auth_pool);
        buf.extend_from_slice(year_pool);
        buf
    }

    fn search_hits(query: &[u8]) -> u32 {
        let mut out = vec![0u8; 4 + RESULT_STRIDE * 16];
        unsafe { search(query.as_ptr(), query.len(), out.as_mut_ptr(), out.len()) }
    }

    fn load_tiny() {
        let blob = build_tiny();
        reset();
        assert_eq!(unsafe { init(blob.as_ptr(), blob.len()) }, 1);
    }

    /// `contains` must agree with an independent oracle over an ASCII corpus.
    #[test]
    fn contains_matches_str_semantics() {
        let hays: &[&str] = &[
            "",
            "a",
            "kbo 3.22",
            "kbo 26.25 (sumerisch-akkadisch-hethitisch)",
            "aaaaaaaa",
            "ababababab",
        ];
        let needles: &[&str] = &[
            "",
            "a",
            "bo",
            "kbo",
            "3.22",
            "sumerisch-akkadisch",
            "zzzz",
            "not-present-at-all",
        ];
        for hay in hays {
            for needle in needles {
                assert_eq!(
                    contains(hay.as_bytes(), needle.as_bytes()),
                    hay.contains(needle),
                    "hay={hay:?} needle={needle:?}"
                );
            }
        }
    }

    #[test]
    fn finds_by_siglum() {
        load_tiny();
        let mut out = vec![0u8; 4 + RESULT_STRIDE * 8];
        let query = b"3.22";
        let n = unsafe { search(query.as_ptr(), query.len(), out.as_mut_ptr(), out.len()) };
        assert_eq!(n, 1);
        // kind == 1 → an item hit, not a whole group
        assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 1);
    }

    #[test]
    fn finds_by_author_pool() {
        load_tiny();
        assert_eq!(search_hits(b"ls"), 2);
    }

    #[test]
    fn finds_by_year_pool() {
        load_tiny();
        assert_eq!(search_hits(b"2021"), 2);
    }

    #[test]
    fn finds_group_by_cth_label() {
        load_tiny();
        let mut out = vec![0u8; 4 + RESULT_STRIDE * 8];
        let query = b"cth 1";
        let n = unsafe { search(query.as_ptr(), query.len(), out.as_mut_ptr(), out.len()) };
        assert_eq!(n, 1);
        // kind == 0 → the whole group matched
        assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn empty_query_returns_every_group() {
        load_tiny();
        assert_eq!(search_hits(b""), 1);
    }

    #[test]
    fn miss_returns_nothing() {
        load_tiny();
        assert_eq!(search_hits(b"zzz"), 0);
    }

    #[test]
    fn search_without_index_returns_zero() {
        reset();
        assert_eq!(search_hits(b"kbo"), 0);
    }

    /// Loading a second index over a live one must succeed and leave a working
    /// index behind — the page re-inits whenever the inventory is rebuilt.
    #[test]
    fn init_over_a_loaded_index_replaces_it() {
        let blob = build_tiny();
        reset();
        assert_eq!(unsafe { init(blob.as_ptr(), blob.len()) }, 1);
        let before = search_hits(b"kbo");
        assert!(before > 0);

        // No `reset` in between: this is the path JavaScript actually takes.
        assert_eq!(unsafe { init(blob.as_ptr(), blob.len()) }, 1);
        assert_eq!(search_hits(b"kbo"), before);
    }

    /// A rejected blob must not take the working index down with it: the page
    /// keeps searching the data it already had.
    #[test]
    fn failed_init_keeps_the_previous_index() {
        load_tiny();
        let before = search_hits(b"kbo");
        assert!(before > 0);

        let garbage = [0u8; 64];
        assert_eq!(unsafe { init(garbage.as_ptr(), garbage.len()) }, 0);
        assert_eq!(
            search_hits(b"kbo"),
            before,
            "a malformed blob must not clear the loaded index"
        );

        assert_eq!(unsafe { init(core::ptr::null(), 8) }, 0);
        assert_eq!(search_hits(b"kbo"), before);
    }

    /// `reset` is idempotent — the JS side calls it before every load.
    #[test]
    fn reset_twice_is_harmless() {
        load_tiny();
        reset();
        reset();
        assert_eq!(search_hits(b"kbo"), 0);
    }

    #[test]
    fn output_buffer_capacity_is_respected() {
        load_tiny();
        // Room for the header and exactly one entry.
        let mut out = vec![0u8; 4 + RESULT_STRIDE];
        let query = b"kbo";
        let n = unsafe { search(query.as_ptr(), query.len(), out.as_mut_ptr(), out.len()) };
        assert_eq!(n, 1);
        assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), 1);
    }

    #[test]
    fn full_search_matches_a_naive_scan() {
        load_tiny();
        let sigs: &[&[u8]] = &[b"kbo 3.22", b"kbo 22.5"];
        let queries: &[&[u8]] = &[b"3.22", b"22.5", b"kbo", b"ls", b"2021", b"cth 1", b"zzz", b"2"];
        for query in queries {
            let expected = if contains(b"cth 1", query) {
                1
            } else {
                sigs.iter()
                    .filter(|sig| {
                        contains(sig, query) || contains(b"ls", query) || contains(b"2021", query)
                    })
                    .count() as u32
            };
            assert_eq!(search_hits(query), expected, "query={query:?}");
        }
    }

    /// Regression: `init` must reject a blob whose item offsets point outside
    /// the siglum pool instead of trapping the whole module.
    #[test]
    fn init_rejects_out_of_range_item_offset() {
        let mut blob = build_tiny();
        // First item's `sig_off` lives right after the header and group table.
        let sig_off_at = HEADER + GROUP_STRIDE;
        blob[sig_off_at..sig_off_at + 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        reset();
        assert_eq!(unsafe { init(blob.as_ptr(), blob.len()) }, 0);
    }

    #[track_caller]
    fn assert_rejected(blob: Vec<u8>, expected: IndexError) {
        match IndexView::parse(blob) {
            Err(actual) => assert_eq!(actual, expected),
            Ok(_) => panic!("expected {expected:?}, but the blob was accepted"),
        }
    }

    #[test]
    fn init_rejects_malformed_blobs() {
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

    #[test]
    fn init_rejects_null_and_empty() {
        assert_eq!(unsafe { init(core::ptr::null(), 8) }, 0);
        let blob = build_tiny();
        assert_eq!(unsafe { init(blob.as_ptr(), 0) }, 0);
    }

    #[test]
    fn alloc_zero_is_null_and_dealloc_is_a_noop() {
        assert!(alloc(0).is_null());
        unsafe { dealloc(core::ptr::null_mut(), 16) };
    }

    #[test]
    fn alloc_round_trip() {
        let n = 64;
        let ptr = alloc(n);
        assert!(!ptr.is_null());
        unsafe {
            core::slice::from_raw_parts_mut(ptr, n).fill(7);
            assert_eq!(core::slice::from_raw_parts(ptr, n)[63], 7);
            dealloc(ptr, n);
        }
    }
}
