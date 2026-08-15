//! Compact inventory search over the TLH2 index.
//!
//! The module is loaded by `src/lib/wasm-search.ts` and answers one question:
//! which CTH groups and which manuscripts inside them contain the query string.
//! Queries and index are both lowercase ASCII-folded on the JavaScript side.
//!
//! This file is the boundary and nothing else — the five `extern "C"` entry
//! points, and the one slot the loaded index lives in. The work behind them is
//! split by what it is responsible for:
//!
//! * [`format`] — the layout, shared by name with the TypeScript builder;
//! * [`index`] — validating a blob into something that can be read without
//!   further checks;
//! * [`search`] — matching a query and writing results back.
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
//! `unsafe` appears only in the five entry points below, where it is
//! unavoidable — everything under them is safe Rust.

mod format;
mod index;
mod search;

#[cfg(test)]
mod fixtures;

use index::IndexView;
use search::run_search;
use std::alloc::{alloc as raw_alloc, dealloc as raw_dealloc, Layout};
use std::cell::RefCell;

thread_local! {
    /// wasm32 is single-threaded; a `RefCell` is enough and costs no lock.
    static INDEX: RefCell<Option<IndexView>> = const { RefCell::new(None) };
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

// ── tests ───────────────────────────────────────────────────────
//
// What the boundary itself has to guarantee: the module survives every bad
// argument, and a refused index leaves the previous one alone. Matching is
// tested in `search`, validation in `index`.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::build_tiny;
    use crate::format::RESULT_STRIDE;

    fn search_hits(query: &[u8]) -> u32 {
        let mut out = vec![0u8; 4 + RESULT_STRIDE * 16];
        unsafe { search(query.as_ptr(), query.len(), out.as_mut_ptr(), out.len()) }
    }

    fn load_tiny() {
        let blob = build_tiny();
        reset();
        assert_eq!(unsafe { init(blob.as_ptr(), blob.len()) }, 1);
    }

    /// Searching after a refused load must answer "nothing", not trap: the
    /// module has to stay usable so a later, valid index can be loaded.
    #[test]
    fn a_refused_index_leaves_the_module_alive() {
        let mut blob = build_tiny();
        blob[crate::format::HEADER + crate::format::GROUP_STRIDE + 5] = 200;
        reset();
        assert_eq!(unsafe { init(blob.as_ptr(), blob.len()) }, 0);
        assert_eq!(search_hits(b"ls"), 0, "no index loaded → no results");

        // The module still works afterwards.
        load_tiny();
        assert!(search_hits(b"ls") > 0, "a valid index still loads and matches");
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
