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
    let auth_bits = matching(index, &index.auth, query);
    let year_bits = matching(index, &index.year, query);

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
            if item_matches(index, item, query, auth_bits, year_bits) {
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
    auth_bits: u64,
    year_bits: u64,
) -> bool {
    bit(auth_bits, item.auth) || bit(year_bits, item.year) || contains(index.sig(item), query)
}

/// Bitset of pool entries containing `query`. Pools hold at most 64 entries,
/// so this is one pass over the pool instead of one per item.
fn matching(index: &IndexView, pool: &[Range<usize>], query: &[u8]) -> u64 {
    let mut bits = 0u64;
    for (i, range) in pool.iter().enumerate() {
        if contains(index.pooled(range), query) {
            bits |= 1u64 << i;
        }
    }
    bits
}

/// Test bit `idx` of a pool bitset.
///
/// `IndexView::parse` already refuses ids past their pool, so `idx` is in range
/// by the time a search runs. The guard stays because the cost of being wrong is
/// not a bad answer but a trap: this module is compiled with `panic = "abort"`,
/// so one overflowing shift would take search off the page entirely.
fn bit(bits: u64, idx: u8) -> bool {
    idx < 64 && bits >> idx & 1 != 0
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

    /// The bit test is total, so even an id that somehow reached a search
    /// cannot overflow the shift.
    #[test]
    fn bit_test_is_total() {
        assert!(bit(0b101, 0));
        assert!(!bit(0b101, 1));
        assert!(bit(0b101, 2));
        assert!(!bit(u64::MAX, 64), "past the bitset, not wrapped to bit 0");
        assert!(!bit(u64::MAX, 255));
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
