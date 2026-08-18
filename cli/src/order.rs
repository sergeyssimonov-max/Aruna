//! Display order for the inventory, and the keys it is computed from.
//!
//! Pure: records in, order out. No archive, no I/O, nothing this module cannot
//! be handed by a test. It lives apart from [`crate::archive`] because its cost
//! is a function of the inventory — how many records, how their sigla are
//! spelled — while the reader's cost is a function of the ZIP. Measuring or
//! replacing one should not mean touching the other, and while they shared a
//! file the benchmark could only report them added together.
//!
//! The order itself is unchanged and deliberately so: it is what both outputs
//! list manuscripts in, and `src/data/inventory.json` is a committed record of
//! it over the whole corpus.

use crate::parse::ManuscriptRecord;

/// Order records for display: by CTH number, then natural-order sigla
/// (`KBo 3.22` before `KBo 22.5`), then editor and year.
///
/// Sequential, like the parsing that feeds it. A thread pool was tried on the
/// parse and removed at 1.07×; inflating the ZIP is ~72 % of the run and a
/// single `ZipArchive` reader cannot be read in parallel, so even free parsing
/// and sorting would buy about 1.4× — under the 1.5–2× a dependency has to earn
/// here. See `PERFORMANCE.md`.
///
/// Takes a slice: the records are permuted where they lie. Building the sorted
/// output as a second `Vec` meant every record — eight `String`s apiece — was
/// moved twice and both copies were live at once, for an inventory that is
/// already in memory and already the right length.
pub fn sort_records(records: &mut [ManuscriptRecord]) {
    sort_by_display_order(records, |record| record)
}

/// The same order, over anything that carries a record.
///
/// The export builds pairs of a record and the archive entry it came from, and
/// has to list them in the order the inventory does. Sorting a second kind of
/// item by copying the comparison would be two descriptions of one order, which
/// is how the two halves of this project have drifted before — so there is one
/// description and [`sort_records`] is a call to it.
pub fn sort_by_display_order<T>(items: &mut [T], record: impl Fn(&T) -> &ManuscriptRecord) {
    let records: Vec<&ManuscriptRecord> = items.iter().map(&record).collect();
    let keys = SortKeys::build(&records);

    let mut order: Vec<u32> = (0..items.len() as u32).collect();
    order.sort_unstable_by(|&a, &b| {
        let (a, b) = (a as usize, b as usize);
        keys.get(a)
            .cmp(keys.get(b))
            .then_with(|| records[a].authorship.cmp(&records[b].authorship))
            .then_with(|| records[a].year.cmp(&records[b].year))
    });

    apply_order(items, &order);
}

/// The primary sort keys of a whole inventory, in two allocations.
///
/// One encoded key per record, all of them end to end in `buf`. The keys used
/// to be a `Vec` of `Num`/`Text` segments per record, which for this corpus is
/// about a hundred thousand `String`s built to be compared a few times each and
/// then dropped.
///
/// A key is compared as bytes and nothing else, so the whole of the primary
/// ordering — catalogue number first, then the siglum read in natural order —
/// is one `memcmp`.
struct SortKeys {
    buf: Vec<u8>,
    /// `(start, end)` into `buf`, one per record, in record order.
    spans: Vec<(u32, u32)>,
}

/// Tag for a run of digits. Below [`TAG_TEXT`] so a number sorts before text at
/// the same position, which is the order the segments were compared in when
/// they were an enum.
const TAG_NUM: u8 = 0x00;
/// Tag for a run of anything else.
///
/// Both tags are below every byte a siglum contains, which is what lets one
/// `memcmp` do the whole comparison: where one key ends a segment and another
/// continues it, the tag is the smaller byte and the shorter key wins — the
/// same answer a lexicographic compare of segment lists gave.
const TAG_TEXT: u8 = 0x01;

impl SortKeys {
    fn build(records: &[&ManuscriptRecord]) -> Self {
        // Two bytes of tag and a handful of content per segment; the sigla in
        // this corpus encode to about 30 bytes. Sized to land near that without
        // pretending to know the archive.
        let mut buf = Vec::with_capacity(records.len() * 32);
        let mut spans = Vec::with_capacity(records.len());

        for record in records {
            let start = buf.len() as u32;
            encode_key(&mut buf, record.cth_num, &record.sigla);
            spans.push((start, buf.len() as u32));
        }

        Self { buf, spans }
    }

    #[inline]
    fn get(&self, i: usize) -> &[u8] {
        let (start, end) = self.spans[i];
        &self.buf[start as usize..end as usize]
    }
}

/// Write the primary key of one record: catalogue number, then its siglum read
/// in natural order.
///
/// The catalogue number goes in big-endian so that comparing the bytes compares
/// the number, and first because it is the primary ordering — `u32::MAX` for a
/// record with no CTH, which is what sends those to the end.
fn encode_key(buf: &mut Vec<u8>, cth_num: u32, sigla: &str) {
    buf.extend_from_slice(&cth_num.to_be_bytes());
    encode_natural(buf, sigla);
}

/// Encode `KBo 3.22+` so that byte order is natural order: text compares as
/// text, and a run of digits compares as the number it spells (`3.22` before
/// `22.5`).
fn encode_natural(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // A run too long for a `u64` counts as zero, which is what parsing
            // it did. Digits are ASCII, so the slice is valid UTF-8 by
            // construction and the number is read straight from the bytes.
            let n: u64 = std::str::from_utf8(&bytes[start..i])
                .ok()
                .and_then(|digits| digits.parse().ok())
                .unwrap_or(0);
            buf.push(TAG_NUM);
            buf.extend_from_slice(&n.to_be_bytes());
        } else {
            let start = i;
            while i < bytes.len() && !bytes[i].is_ascii_digit() {
                i += 1;
            }
            buf.push(TAG_TEXT);
            // Lowercased a byte at a time into the buffer the key is being
            // built in. `to_ascii_lowercase` on the string would allocate one
            // more `String` per record and leave the non-ASCII bytes — the
            // Turkish dotted capitals a few sigla carry — exactly as they are
            // here anyway.
            buf.extend(bytes[start..i].iter().map(u8::to_ascii_lowercase));
        }
    }
}

/// Reorder `items` so that position `rank` holds what `order[rank]` points at.
///
/// Cycle-following: each record is swapped into place and never copied again,
/// and no second copy of the inventory is needed to hold the result.
///
/// The two directions a permutation can be written in are easy to confuse, and
/// getting it backwards produces a plausible-looking order rather than an
/// error. Sorting yields *what belongs at each rank*; swapping in place needs
/// *where each element belongs*. The inversion below is that turn, and it costs
/// four bytes per record against the eight `String`s a second `Vec` would move.
fn apply_order<T>(items: &mut [T], order: &[u32]) {
    debug_assert_eq!(items.len(), order.len());

    let mut destination: Vec<u32> = vec![0; order.len()];
    for (rank, &from) in order.iter().enumerate() {
        destination[from as usize] = rank as u32;
    }

    for i in 0..items.len() {
        while destination[i] as usize != i {
            let j = destination[i] as usize;
            items.swap(i, j);
            destination.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ordering this module replaced, kept as the thing to agree with.
    ///
    /// It is the implementation the committed catalog was built by, so a change
    /// in the encoded key that this does not object to is a change in the order
    /// the inventory is published in.
    mod reference {
        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        pub enum NatPart {
            Num(u64),
            Text(String),
        }

        pub fn natural_sigla_key(s: &str) -> Vec<NatPart> {
            let lower = s.to_ascii_lowercase();
            let b = lower.as_bytes();
            let mut out = Vec::new();
            let mut i = 0usize;
            while i < b.len() {
                if b[i].is_ascii_digit() {
                    let start = i;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    let n: u64 = lower[start..i].parse().unwrap_or(0);
                    out.push(NatPart::Num(n));
                } else {
                    let start = i;
                    while i < b.len() && !b[i].is_ascii_digit() {
                        i += 1;
                    }
                    out.push(NatPart::Text(lower[start..i].to_string()));
                }
            }
            out
        }
    }

    /// Sigla shaped like the ones the corpus actually publishes, plus the edges
    /// that decide whether an encoding is right: numbers of different widths,
    /// leading zeros, a run too long for a `u64`, empty text, and the non-ASCII
    /// capitals a handful of sigla carry.
    const SIGLA: &[&str] = &[
        "KBo 3.22",
        "KBo 22.5",
        "KBo 3.22+",
        "KBo 3.2",
        "KBo 7.30",
        "KBo 12.18",
        "KUB 2.1",
        "KUB 26.71",
        "kbo 3.22",
        "KBo 003.22",
        "KBo 3.022",
        "ABoT 1.54",
        "İK 174-66",
        "İÇK 174-66",
        "Bo 1234",
        "",
        "—",
        "99999999999999999999999999",
        "1",
        "01",
        "A",
        "A1",
        "1A",
        "KBo",
        "KBo ",
    ];

    fn encoded(sigla: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_natural(&mut buf, sigla);
        buf
    }

    /// Every pair, both ways: the encoded keys must order exactly as the
    /// segment lists did.
    #[test]
    fn the_encoded_key_orders_sigla_the_way_the_segment_list_did() {
        for a in SIGLA {
            for b in SIGLA {
                let want = reference::natural_sigla_key(a).cmp(&reference::natural_sigla_key(b));
                let got = encoded(a).cmp(&encoded(b));
                assert_eq!(
                    got, want,
                    "encoded order disagrees with the segment list for {a:?} vs {b:?}"
                );
            }
        }
    }

    #[test]
    fn numeric_runs_compare_as_integers() {
        assert!(encoded("KBo 3.22") < encoded("KBo 22.5"));
        assert!(encoded("KBo 7.30") < encoded("KBo 12.18"));
        assert!(encoded("KUB 2.1") < encoded("KUB 26.71"));
    }

    /// The catalogue number outranks the siglum, and a record without one goes
    /// last however its siglum reads.
    #[test]
    fn the_catalogue_number_is_the_primary_key() {
        let mut early = Vec::new();
        encode_key(&mut early, 5, "ZZZ 999");
        let mut late = Vec::new();
        encode_key(&mut late, 786, "AAA 1");
        let mut none = Vec::new();
        encode_key(&mut none, u32::MAX, "AAA 1");

        assert!(early < late);
        assert!(late < none);
    }

    fn rec(sigla: &str, cth_num: u32, editor: &str, year: &str) -> ManuscriptRecord {
        ManuscriptRecord {
            title: sigla.into(),
            sigla: sigla.into(),
            cth: None,
            cth_num,
            authorship: editor.into(),
            year: year.into(),
            lang: "Hit".into(),
            inv: "—".into(),
            corpus: "HFR".into(),
        }
    }

    #[test]
    fn records_are_ordered_by_cth_then_sigla_then_editor_then_year() {
        let mut records = vec![
            rec("KBo 22.5", 1, "AA", "2000"),
            rec("KBo 3.22", 1, "AA", "2000"),
            rec("KBo 1.1", 786, "AA", "2000"),
            rec("KBo 1.1", 1, "BB", "1999"),
            rec("KBo 1.1", 1, "AA", "2001"),
            rec("KBo 1.1", 1, "AA", "1998"),
        ];
        sort_records(&mut records);

        let seen: Vec<_> = records
            .iter()
            .map(|r| {
                (
                    r.cth_num,
                    r.sigla.as_str(),
                    r.authorship.as_str(),
                    r.year.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            vec![
                (1, "KBo 1.1", "AA", "1998"),
                (1, "KBo 1.1", "AA", "2001"),
                (1, "KBo 1.1", "BB", "1999"),
                (1, "KBo 3.22", "AA", "2000"),
                (1, "KBo 22.5", "AA", "2000"),
                (786, "KBo 1.1", "AA", "2000"),
            ]
        );
    }

    #[test]
    fn sorting_nothing_and_sorting_one_are_not_special_cases() {
        let mut none: Vec<ManuscriptRecord> = Vec::new();
        sort_records(&mut none);
        assert!(none.is_empty());

        let mut one = vec![rec("KBo 1.1", 1, "AA", "2000")];
        sort_records(&mut one);
        assert_eq!(one.len(), 1);
    }

    /// The permutation is applied by swapping in place, which is the part of
    /// this module that could quietly lose, duplicate or misplace a record —
    /// and the direction of a permutation is exactly the kind of thing that
    /// looks right while being backwards, so the expectation is spelled out
    /// against a plain gather rather than against another permutation.
    #[test]
    fn a_permutation_puts_every_element_where_the_order_says() {
        for order in [
            // One long cycle: the shape a naive implementation loops on.
            (0..64).map(|i| (i + 1) % 64).collect::<Vec<u32>>(),
            // Reversal, the identity, and something irregular.
            (0..9).rev().collect(),
            (0..9).collect(),
            vec![2, 0, 1],
            vec![3, 1, 0, 2],
            vec![0],
            vec![],
        ] {
            let original: Vec<u32> = (0..order.len() as u32).collect();
            let expected: Vec<u32> = order.iter().map(|&i| original[i as usize]).collect();

            let mut items = original.clone();
            apply_order(&mut items, &order);
            assert_eq!(items, expected, "order {order:?}");
        }
    }
}
