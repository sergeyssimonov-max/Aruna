//! Heuristic parser for TLHdig / AOxml manuscript documents.
//!
//! Tags and attributes are located with [`crate::xml_scan`]: no regex, no
//! full-DOM parse, only the header window. Missing fields → `—`.
//!
//! The work divides three ways, and the files follow the division:
//!
//! * this module owns the record, and the windows a document is read through —
//!   the decisions every field depends on;
//! * [`fields`] pulls one column each out of those windows;
//! * [`classify`] answers a different question entirely — whether an archive
//!   entry is a manuscript at all — and is the only part [`crate::archive`]
//!   consults before deciding to read an entry.

mod classify;
mod fields;
#[cfg(test)]
mod fixtures;

pub use classify::{is_manuscript_xml, looks_like_manuscript};

use fields::{
    extract_corpus, extract_cth, extract_editor_and_year, extract_inv, extract_lang, extract_sigla,
    extract_year_fallback,
};

use crate::xml_scan::{find_close_tag, find_open_tag};

/// Placeholder for missing fields (en-dash per specification).
pub const MISSING: &str = "—";

/// One inventory row extracted from a manuscript XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManuscriptRecord {
    /// Display title: sigla + CTH, e.g. `KBo 17.86+ · CTH 786`.
    pub title: String,
    /// Publication siglum only (for secondary sort within a CTH group).
    pub sigla: String,
    /// Catalogue group label, e.g. `CTH 547`. `None` if unknown.
    pub cth: Option<String>,
    /// Numeric CTH key for ordering (`547` from `CTH 547`); `u32::MAX` if missing.
    pub cth_num: u32,
    /// Modern editor / transliterator (initials or full name).
    pub authorship: String,
    /// Edition / digitalisation year.
    pub year: String,
    /// Language codes the text uses, most-used first (`Hit`, `Hit, Hur`, …).
    pub lang: String,
    /// Museum / excavation inventory number (`AO:InvNr`), if any.
    pub inv: String,
    /// Edition series / corpus from path (`HFR`, `TLH`, `HAnn`, …).
    pub corpus: String,
}

/// Only the leading bytes of each XML are needed — AOHeader ends within a few KiB.
pub const HEADER_READ_LIMIT: usize = 16 * 1024;

/// Truncate `s` to at most `max` bytes on a UTF-8 char boundary (never panics).
pub fn truncate_on_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut i = max;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    &s[..i]
}

/// Parse a single XML document given its archive-relative path and raw text.
///
/// Two windows are handed to the field extractors and every extractor takes the
/// same pair: `header` is the AOHeader block, `window` the leading bytes of the
/// document. Which one a field prefers is a property of that field — the
/// inventory number, for one, is not in the header at all — so the choice lives
/// with the extractor rather than here.
pub fn parse_manuscript(path: &str, xml: &str) -> ManuscriptRecord {
    // Restrict work to the header region; bodies can be hundreds of KiB of cuneiform.
    let window = truncate_on_char_boundary(xml, HEADER_READ_LIMIT);
    let header = extract_header_slice(window);

    let sigla = extract_sigla(header, window, path);
    let cth = extract_cth(header, window, path);
    let (authorship, year_from_editor) = extract_editor_and_year(header);
    let year = year_from_editor
        .or_else(|| extract_year_fallback(header, window))
        .unwrap_or_else(|| MISSING.to_string());

    let cth_num = cth
        .as_ref()
        .and_then(|c| parse_cth_num(c))
        .unwrap_or(u32::MAX);

    ManuscriptRecord {
        title: format_title(&sigla, &cth),
        sigla: if sigla.is_empty() {
            MISSING.to_string()
        } else {
            sigla
        },
        cth,
        cth_num,
        authorship: authorship.unwrap_or_else(|| MISSING.to_string()),
        year,
        lang: extract_lang(window),
        inv: extract_inv(header, window),
        corpus: extract_corpus(path),
    }
}

/// Parse `CTH 547` / `CTH 12.1` → integer major number for sort (547 / 12).
#[inline]
pub fn parse_cth_num(cth: &str) -> Option<u32> {
    let b = cth.as_bytes();
    // skip non-digits
    let mut i = 0usize;
    while i < b.len() && !b[i].is_ascii_digit() {
        i += 1;
    }
    if i >= b.len() {
        return None;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    std::str::from_utf8(&b[start..i]).ok()?.parse().ok()
}

/// Sort key label for grouping (stable display).
#[inline]
pub fn group_label(rec: &ManuscriptRecord) -> &str {
    rec.cth.as_deref().unwrap_or(MISSING)
}

/// The CTH groups, as consecutive runs of records sharing a label.
///
/// A group is a run rather than a bucket because the records arrive sorted
/// from [`crate::archive::sort_records`], and both outputs — the HTML and the
/// site's catalog — must list manuscripts in that one order. Collecting into a
/// map would lose it.
///
/// Both writers used to carry their own copy of this loop.
pub fn group_runs(records: &[ManuscriptRecord]) -> impl Iterator<Item = &[ManuscriptRecord]> {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= records.len() {
            return None;
        }
        let label = group_label(&records[start]);
        let mut end = start + 1;
        while end < records.len() && group_label(&records[end]) == label {
            end += 1;
        }
        let run = &records[start..end];
        start = end;
        Some(run)
    })
}

fn format_title(sigla: &str, cth: &Option<String>) -> String {
    match cth {
        Some(c) if !sigla.is_empty() && sigla != MISSING => format!("{sigla} · {c}"),
        Some(c) => c.clone(),
        None if !sigla.is_empty() && sigla != MISSING => sigla.to_string(),
        None => MISSING.to_string(),
    }
}

/// How much of a malformed document to treat as its header.
const HEADER_FALLBACK_LIMIT: usize = 8 * 1024;

/// Prefer the AOHeader / teiHeader block; otherwise the first 8 KiB.
fn extract_header_slice(xml: &str) -> &str {
    for tag in [b"AOHeader".as_slice(), b"teiHeader"] {
        if let Some(header) = header_block(xml, tag) {
            return header;
        }
    }
    truncate_on_char_boundary(xml, HEADER_FALLBACK_LIMIT)
}

/// Text between `<tag>` and `</tag>`.
///
/// The content start sits just past an ASCII `>` and the close tag on an ASCII
/// `<`, so both are character boundaries. Only the malformed-input fallback ends
/// at an arbitrary byte offset, which is why it goes through
/// [`truncate_on_char_boundary`] — slicing there directly used to panic on
/// cuneiform.
fn header_block<'a>(xml: &'a str, tag: &[u8]) -> Option<&'a str> {
    let (_, content) = find_open_tag(xml.as_bytes(), tag)?;
    match find_close_tag(xml.as_bytes(), content, tag) {
        Some(close) => xml.get(content..close),
        None => Some(truncate_on_char_boundary(
            xml.get(content..)?,
            HEADER_FALLBACK_LIMIT,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::SAMPLE_FULL;
    use super::*;

    /// TLHdig keeps the inventory number outside the header — `<AO:InvNr>` sits
    /// in `<AO:Manuscripts>` in the body. Searching the header alone found it in
    /// no document at all, so every record in the corpus carried the dash and
    /// museum numbers vanished from search.
    #[test]
    fn inventory_number_is_found_outside_the_header() {
        let xml = r#"<AOxml xmlns:AO="http://hethiter.net/ns/AO/1.0">
<AOHeader><docID>KBo 3.22</docID></AOHeader>
<AO:Manuscripts> <AO:TxtPubl>KBo 3.22</AO:TxtPubl> <AO:InvNr>VAT 7479</AO:InvNr> </AO:Manuscripts>
<body>…</body></AOxml>"#;
        let rec = parse_manuscript("CTH 1_XML_HAnn/KBo 3.22.xml", xml);
        assert_eq!(rec.inv, "VAT 7479");
    }

    /// The header still wins when it carries one, and a document without the
    /// tag anywhere reports the missing-value dash rather than empty text.
    #[test]
    fn inventory_number_prefers_the_header_and_tolerates_absence() {
        let in_header = r#"<AOxml><AOHeader><docID>X</docID><InvNr>Bo 1234</InvNr></AOHeader>
<AO:Manuscripts><AO:InvNr>VAT 9999</AO:InvNr></AO:Manuscripts></AOxml>"#;
        assert_eq!(parse_manuscript("CTH 1_XML/X.xml", in_header).inv, "Bo 1234");

        let absent = r#"<AOxml><AOHeader><docID>X</docID></AOHeader><body>t</body></AOxml>"#;
        assert_eq!(parse_manuscript("CTH 1_XML/X.xml", absent).inv, MISSING);
    }

    #[test]
    fn parses_full_aoxml_with_uebern() {
        let r = parse_manuscript("TLH/CTH 786_XML_HFR/KBo 17.86+.xml", SAMPLE_FULL);
        assert_eq!(r.title, "KBo 17.86+ · CTH 786");
        assert_eq!(r.authorship, "FB");
        assert_eq!(r.year, "2017");
    }

    #[test]
    fn missing_fields_become_dash() {
        let xml = r#"<root><note>nothing useful</note></root>"#;
        let r = parse_manuscript("orphan/unknown.xml", xml);
        assert_eq!(r.title, "unknown");
        assert_eq!(r.authorship, MISSING);
        assert_eq!(r.year, MISSING);
    }

    #[test]
    fn completely_empty_xml() {
        let r = parse_manuscript("x/y/z.xml", "");
        assert_eq!(r.title, "z");
        assert_eq!(r.authorship, MISSING);
        assert_eq!(r.year, MISSING);
    }

    #[test]
    fn malformed_unclosed_tags_still_yield_sigla() {
        let xml = r#"<AOxml><AOHeader><docID>KUB 23.117<docID><meta>
        <creation-date date="2021-09-27T15:06:47"/>
        </meta>"#;
        let r = parse_manuscript("CTH 17_XML_HAnn/KUB 23.117.xml", xml);
        assert!(r.title.contains("CTH 17") || r.title.contains("KUB"));
        assert_eq!(r.year, "2021");
    }

    #[test]
    fn cth_from_neu_attribute() {
        let xml = r#"<AOxml><AOHeader><docID>KBo 55.173</docID>
        <neu><cth editor="SM" date="2019-08-08" alt="CTH 500" neu="CTH 786"/></neu>
        </AOHeader></AOxml>"#;
        let r = parse_manuscript("other/KBo 55.173.xml", xml);
        assert_eq!(r.title, "KBo 55.173 · CTH 786");
        assert_eq!(r.authorship, "SM");
        assert_eq!(r.year, "2019");
    }

    #[test]
    fn full_name_editor() {
        let xml = r#"<AOxml><AOHeader><docID>ABoT 1.54</docID>
        <annot editor="James Burgin" date="2024-11-12T18:48:28.002Z"/>
        </AOHeader></AOxml>"#;
        let r = parse_manuscript("CTH 249_XML_PTAC/ABoT 1.54.xml", xml);
        assert_eq!(r.authorship, "James Burgin");
        assert_eq!(r.year, "2024");
        assert!(r.title.contains("CTH 249"));
    }

    #[test]
    fn tei_like_header() {
        let xml = r#"<?xml version="1.0"?>
        <TEI><teiHeader>
          <titleStmt><title>Bo 1234</title>
          <author>Elisabeth Rieken</author>
          <respStmt><name>Daniel Schwemer</name></respStmt>
          </titleStmt>
          <publicationStmt><date>2019</date></publicationStmt>
        </teiHeader></TEI>"#;
        let r = parse_manuscript("CTH 100_XML_X/Bo 1234.xml", xml);
        assert!(r.title.starts_with("Bo 1234"));
        assert!(r.authorship == "Elisabeth Rieken" || r.authorship == "Daniel Schwemer");
        assert_eq!(r.year, "2019");
    }

    #[test]
    fn skips_auto_editor() {
        let xml = r#"<AOHeader><docID>X 1</docID>
        <annot editor="auto" date="2020-01-01"/>
        <kor editor="SG" date="2021-05-05"/>
        </AOHeader>"#;
        let r = parse_manuscript("CTH 1_XML/X 1.xml", xml);
        assert_eq!(r.authorship, "SG");
        assert_eq!(r.year, "2021");
    }

    #[test]
    fn unicode_filename_and_sigla() {
        let xml = r#"<AOHeader><docID>İK 174-66</docID>
        <creation-date date="2023-07-26T16:24:53"/>
        </AOHeader>"#;
        let r = parse_manuscript("CTH 222_XML_TLH/İÇK 174-66.xml", xml);
        assert!(r.title.contains("İK 174-66"));
        assert!(r.title.contains("CTH 222"));
        assert_eq!(r.year, "2023");
    }

    /// The newer TLH files name the person in `author=`, on an `<author>`
    /// element that has no text of its own. Reading only `editor=` left 549
    /// manuscripts of this shape with a dash where the document said
    /// "Daniel Schwemer".
    #[test]
    fn the_editor_may_be_named_in_an_author_attribute() {
        let xml = r#"<AOxml><AOHeader><docID>KBo 71.260</docID><meta>
        <creation-date date="2026-05-11"/>
        <author date="2026-05-11" author="Daniel Schwemer"/>
        </meta></AOHeader></AOxml>"#;
        let r = parse_manuscript("CTH 694_XML_TLH/KBo 71.260.xml", xml);
        assert_eq!(r.authorship, "Daniel Schwemer");
        assert_eq!(r.year, "2026", "the date beside the name, not another one");
    }

    /// An explicit transliteration role still outranks it: `<uebern>` is who
    /// made this edition, `<author>` the fallback for documents with no roles.
    #[test]
    fn a_transliteration_role_outranks_an_author_attribute() {
        let xml = r#"<AOHeader><docID>X</docID>
        <author date="2026-05-11" author="Daniel Schwemer"/>
        <uebern editor="FB" date="2017-03-28" src="MZ"/>
        </AOHeader>"#;
        let r = parse_manuscript("CTH 1_XML/X.xml", xml);
        assert_eq!(r.authorship, "FB");
        assert_eq!(r.year, "2017");
    }

    /// `src=` sits beside `editor=` on `<uebern>` and says where the
    /// transliteration was taken over from — not who did it. Taking it would
    /// credit 27% of the corpus to publication sigla like `MZ`.
    #[test]
    fn the_source_of_a_takeover_is_not_its_editor() {
        let xml = r#"<AOHeader><docID>X</docID>
        <uebern editor="" date="2017-03-28" src="MZ"/>
        </AOHeader>"#;
        assert_eq!(parse_manuscript("CTH 1_XML/X.xml", xml).authorship, MISSING);
    }

    #[test]
    fn trlst_role_preferred_over_annot() {
        let xml = r#"<AOHeader><docID>KBo 29.170</docID>
        <annot editor="XX" date="2022-01-01"/>
        <trlst editor="TS" date="2016-11-24"/>
        </AOHeader>"#;
        let r = parse_manuscript("CTH 692_XML_HFR/KBo 29.170.xml", xml);
        assert_eq!(r.authorship, "TS");
        assert_eq!(r.year, "2016");
    }

    /// Regression: an unclosed `<AOHeader>` makes the header fallback cut the
    /// window at a fixed byte offset. When that offset lands inside a multi-byte
    /// character (cuneiform is 4 bytes), slicing must not panic.
    #[test]
    fn unclosed_header_with_cuneiform_at_cut_offset() {
        const OPEN: &str = "<AOHeader>";
        let mut xml = String::from(OPEN);
        xml.push_str(&"x".repeat(8192 - 2));
        xml.push_str(&"𒀀".repeat(64));
        assert!(!xml.is_char_boundary(OPEN.len() + 8192));

        let r = parse_manuscript("CTH 5_XML_HFR/broken.xml", &xml);
        assert_eq!(r.title, "broken · CTH 5");
    }

    #[test]
    fn large_body_does_not_break_header_parse() {
        let mut xml = String::from(SAMPLE_FULL);
        xml.push_str(&"x".repeat(500_000));
        let r = parse_manuscript("CTH 786_XML_HFR/KBo 17.86+.xml", &xml);
        assert_eq!(r.authorship, "FB");
        assert_eq!(r.year, "2017");
    }
}
