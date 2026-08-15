//! Answering a query against a parsed index.
//!
//! Nothing here validates anything: [`crate::index`] already did, so this is
//! only the matching and the writing of results. The matcher is a plain
//! substring test — see the note on performance in the crate documentation
//! before replacing it with something cleverer.

use crate::format::RESULT_STRIDE;
use crate::index::{IndexView, Item};
use std::ops::Range;

/// Match `query` against the index, writing results into `out`.
///
/// Returns the number of entries written, which is also written into the first
/// four bytes of `out`. An empty query matches every group.
///
/// Results are truncated to what `out` can hold rather than reported as an
/// error: the caller sizes the buffer for the whole inventory, and a query that
/// somehow matched more than that is better answered partially than not at all.
pub fn run_search(index: &IndexView, query: &[u8], out: &mut [u8]) -> u32 {
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
    let auth_hits = PoolHits::of(index, &index.auth, query);
    let year_hits = PoolHits::of(index, &index.year, query);

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
            if item_matches(index, item, query, &auth_hits, &year_hits) {
                write_entry(out, count, gi as u32, 1, offset as u32);
                count += 1;
            }
        }
    }

    write_count(out, count);
    count as u32
}

/// Whether one manuscript answers the query — by its metadata or by its text.
fn item_matches(
    index: &IndexView,
    item: &Item,
    query: &[u8],
    auth_hits: &PoolHits,
    year_hits: &PoolHits,
) -> bool {
    auth_hits.has(item.auth) || year_hits.has(item.year) || contains(index.sig(item), query)
}

/// Which entries of one pool contain the query — one bit each.
///
/// Metadata is deduplicated, so the query is compared against each distinct
/// author or year once and every item then costs a bit test instead of a
/// substring search.
///
/// Sized to the pool rather than to a machine word. It was a single `u64`,
/// which capped both pools at 64 entries — a limit belonging to the matcher and
/// not to the format, whose ids are `u8`. Two words cover the 255 the container
/// can express, and the allocation is two `Vec`s per query against ~25 000
/// substring tests.
struct PoolHits(Vec<u64>);

impl PoolHits {
    fn of(index: &IndexView, pool: &[Range<usize>], query: &[u8]) -> Self {
        let mut words = vec![0u64; pool.len().div_ceil(64)];
        for (i, range) in pool.iter().enumerate() {
            if contains(index.pooled(range), query) {
                words[i / 64] |= 1u64 << (i % 64);
            }
        }
        Self(words)
    }

    /// Whether pool entry `idx` matched.
    ///
    /// `IndexView::parse` already refuses ids past their pool, so `idx` is in
    /// range by the time a search runs. The bounds check stays because the cost
    /// of being wrong is not a bad answer but a trap: this module is compiled
    /// with `panic = "abort"`, so one out-of-range index would take search off
    /// the page entirely.
    fn has(&self, idx: u8) -> bool {
        let idx = idx as usize;
        self.0
            .get(idx / 64)
            .is_some_and(|word| word >> (idx % 64) & 1 != 0)
    }
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

fn write_count(out: &mut [u8], count: usize) {
    out[0..4].copy_from_slice(&(count as u32).to_le_bytes());
}

fn write_entry(out: &mut [u8], idx: usize, gi: u32, kind: u32, extra: u32) {
    let at = 4 + idx * RESULT_STRIDE;
    out[at..at + 4].copy_from_slice(&gi.to_le_bytes());
    out[at + 4..at + 8].copy_from_slice(&kind.to_le_bytes());
    out[at + 8..at + 12].copy_from_slice(&extra.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::build_tiny;

    /// Search the fixture index directly, with room for `cap` entries.
    fn search_tiny(query: &[u8], cap: usize) -> (u32, Vec<u8>) {
        let index = IndexView::parse(build_tiny()).expect("the fixture parses");
        let mut out = vec![0u8; 4 + RESULT_STRIDE * cap];
        let n = run_search(&index, query, &mut out);
        (n, out)
    }

    fn hits(query: &[u8]) -> u32 {
        search_tiny(query, 16).0
    }

    /// Third word of an entry: 0 = the whole group matched, 1 = one manuscript.
    fn kind_of_first(out: &[u8]) -> u32 {
        u32::from_le_bytes(out[8..12].try_into().unwrap())
    }

    #[test]
    fn finds_by_siglum() {
        let (n, out) = search_tiny(b"3.22", 8);
        assert_eq!(n, 1);
        assert_eq!(kind_of_first(&out), 1, "an item hit, not a whole group");
    }

    #[test]
    fn finds_by_author_pool() {
        assert_eq!(hits(b"ls"), 2);
    }

    #[test]
    fn finds_by_year_pool() {
        assert_eq!(hits(b"2021"), 2);
    }

    #[test]
    fn finds_group_by_cth_label() {
        let (n, out) = search_tiny(b"cth 1", 8);
        assert_eq!(n, 1);
        assert_eq!(kind_of_first(&out), 0, "the whole group matched");
    }

    #[test]
    fn empty_query_returns_every_group() {
        assert_eq!(hits(b""), 1);
    }

    #[test]
    fn miss_returns_nothing() {
        assert_eq!(hits(b"zzz"), 0);
    }

    #[test]
    fn output_buffer_capacity_is_respected() {
        // Room for the header and exactly one entry.
        let (n, out) = search_tiny(b"kbo", 1);
        assert_eq!(n, 1);
        assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), 1);
    }

    #[test]
    fn full_search_matches_a_naive_scan() {
        let sigs: &[&[u8]] = &[b"kbo 3.22", b"kbo 22.5"];
        let queries: &[&[u8]] = &[
            b"3.22", b"22.5", b"kbo", b"ls", b"2021", b"cth 1", b"zzz", b"2",
        ];
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
            assert_eq!(hits(query), expected, "query={query:?}");
        }
    }

    /// The bit test is total: an id past the pool answers "no match" rather
    /// than shifting out of range, whatever reached it.
    #[test]
    fn the_bit_test_is_total() {
        let one_word = PoolHits(vec![0b101]);
        assert!(one_word.has(0));
        assert!(!one_word.has(1));
        assert!(one_word.has(2));
        assert!(!one_word.has(64), "past the words held, not wrapped to bit 0");
        assert!(!one_word.has(255));

        assert!(!PoolHits(vec![]).has(0), "an empty pool matches nothing");
    }

    /// Entries beyond the first word must be reachable — this is what the
    /// single `u64` could not do, and why the pools were capped at 64.
    #[test]
    fn a_pool_wider_than_one_word_is_addressable() {
        let mut words = vec![0u64; 4];
        for idx in [0usize, 63, 64, 65, 127, 128, 254] {
            words[idx / 64] |= 1u64 << (idx % 64);
        }
        let hits = PoolHits(words);
        for idx in [0u8, 63, 64, 65, 127, 128, 254] {
            assert!(hits.has(idx), "entry {idx} should match");
        }
        for idx in [1u8, 62, 66, 129, 253, 255] {
            assert!(!hits.has(idx), "entry {idx} should not match");
        }
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
}
