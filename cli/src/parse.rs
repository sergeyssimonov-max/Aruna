//! Heuristic parser for TLHdig / AOxml manuscript documents.
//!
//! Tags and attributes are located with [`crate::xml_scan`]: no regex, no
//! full-DOM parse, only the header window. Missing fields → `—`.

use crate::xml_scan::{
    attr_value, eq_ci, find_ci, find_close_tag, find_cth_number, find_open_tag, find_year,
    for_each_start_tag, strip_tags_bytes, tag_text,
};
use std::collections::HashMap;
use std::path::Path;

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
    /// Dominant text language code (Hit, Hur, Akk, …).
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

fn extract_sigla(header: &str, xml: &str, path: &str) -> String {
    if let Some(v) = first_tag_text(header, b"docID").or_else(|| first_tag_text(xml, b"docID")) {
        let t = normalize_ws(&v);
        if !t.is_empty() {
            return t;
        }
    }
    if let Some(v) = first_tag_text(header, b"TxtPubl").or_else(|| first_tag_text(xml, b"TxtPubl"))
    {
        let primary = v.split('{').next().unwrap_or(&v);
        let t = normalize_ws(primary);
        if !t.is_empty() {
            return t;
        }
    }
    if let Some(v) = first_tag_text(header, b"title") {
        let t = normalize_ws(&v);
        if !t.is_empty() {
            return t;
        }
    }
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| MISSING.to_string())
}

fn extract_cth(header: &str, xml: &str, path: &str) -> Option<String> {
    if let Some(n) = find_cth_number(path.as_bytes()) {
        return Some(format!("CTH {}", path_utf8(n)));
    }
    // <cth neu="CTH 786" …>
    let mut found: Option<String> = None;
    for_each_start_tag(header.as_bytes(), |local, attrs| {
        if !eq_local(local, b"cth") {
            return false;
        }
        for key in [b"neu".as_slice(), b"alt", b"n"] {
            if let Some(v) = attr_value(attrs, key) {
                if let Some(num) = find_cth_number(v) {
                    found = Some(format!("CTH {}", path_utf8(num)));
                    return true;
                }
                // value may be bare number
                if !v.is_empty() && v[0].is_ascii_digit() {
                    found = Some(format!("CTH {}", path_utf8(v)));
                    return true;
                }
            }
        }
        false
    });
    if found.is_some() {
        return found;
    }
    if let Some(n) = find_cth_number(header.as_bytes()) {
        return Some(format!("CTH {}", path_utf8(n)));
    }
    let early = xml.as_bytes().get(..4096.min(xml.len())).unwrap_or(b"");
    find_cth_number(early).map(|n| format!("CTH {}", path_utf8(n)))
}

#[inline]
fn eq_local(a: &[u8], b: &[u8]) -> bool {
    eq_ci(a, b)
}

#[inline]
fn path_utf8(b: &[u8]) -> &str {
    std::str::from_utf8(b).unwrap_or("")
}

/// Priority for authorship roles (Übernahme / transliteration first).
const EDITOR_ROLE_PRIORITY: &[&[u8]] = &[
    b"uebern",
    b"trlst",
    b"transliteration",
    b"editor",
    b"resp",
    b"name",
    b"kor1kf",
    b"kor",
    b"kor2",
    b"annot",
    b"val",
    b"format",
    b"kolon",
    b"cth",
];

fn extract_editor_and_year(header: &str) -> (Option<String>, Option<String>) {
    // TEI-like name / author elements first (full names).
    for tag in [b"name".as_slice(), b"persName", b"author"] {
        if let Some(v) = first_tag_text(header, tag) {
            let n = normalize_ws(&v);
            if !n.is_empty() && !is_auto_editor(&n) {
                let year = find_year(header.as_bytes()).map(|y| path_utf8(&y).to_string());
                return (Some(n), year);
            }
        }
    }

    let mut best: Option<(usize, String, Option<String>)> = None;

    for_each_start_tag(header.as_bytes(), |local, attrs| {
        let Some(ed_raw) = attr_value(attrs, b"editor") else {
            return false;
        };
        let editor = normalize_ws(path_utf8(ed_raw));
        if editor.is_empty() || is_auto_editor(&editor) {
            return false;
        }
        let year = attr_value(attrs, b"date").and_then(|d| year_from_date_value(path_utf8(d)));
        let prio = EDITOR_ROLE_PRIORITY
            .iter()
            .position(|r| eq_local(local, r))
            .unwrap_or(EDITOR_ROLE_PRIORITY.len() + 10);

        // Special-case übern (UTF-8) — local name may be multi-byte; already handled as uebern.
        let replace = match &best {
            None => true,
            Some((bp, _, by)) => prio < *bp || (prio == *bp && by.is_none() && year.is_some()),
        };
        if replace {
            best = Some((prio, editor, year));
        }
        false // continue scanning for better priority
    });

    match best {
        Some((_, ed, yr)) => (Some(ed), yr),
        None => (None, None),
    }
}

fn is_auto_editor(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l == "auto" || l == "system" || l == "hfr"
}

fn extract_year_fallback(header: &str, xml: &str) -> Option<String> {
    // Prefer date= on known meta tags.
    let mut year: Option<String> = None;
    for_each_start_tag(header.as_bytes(), |local, attrs| {
        let interesting = eq_local(local, b"creation-date")
            || eq_local(local, b"AOxml-creation")
            || eq_local(local, b"kor2")
            || eq_local(local, b"kor1")
            || eq_local(local, b"date");
        if !interesting {
            return false;
        }
        if let Some(d) = attr_value(attrs, b"date").and_then(|d| year_from_date_value(path_utf8(d)))
        {
            year = Some(d);
            return true;
        }
        // TEI <date>2019</date> text content handled below
        false
    });
    if year.is_some() {
        return year;
    }
    // <date>…</date> element text
    if let Some(t) = first_tag_text(header, b"date") {
        if let Some(y) = year_from_date_value(&t).or_else(|| {
            find_year(t.as_bytes()).map(|y| path_utf8(&y).to_string())
        }) {
            return Some(y);
        }
    }
    find_year(header.as_bytes())
        .or_else(|| find_year(xml.as_bytes().get(..2048.min(xml.len())).unwrap_or(b"")))
        .map(|y| path_utf8(&y).to_string())
}

fn year_from_date_value(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.len() >= 4 && t.as_bytes()[0].is_ascii_digit() {
        let y = &t.as_bytes()[..4];
        if y.iter().all(|c| c.is_ascii_digit()) {
            let n: u32 = path_utf8(y).parse().ok()?;
            if (1900..=2100).contains(&n) {
                return Some(path_utf8(y).to_string());
            }
        }
    }
    None
}

fn first_tag_text(hay: &str, local: &[u8]) -> Option<String> {
    let raw = tag_text(hay.as_bytes(), local)?;
    let stripped = strip_tags_bytes(raw);
    let s = String::from_utf8_lossy(&stripped);
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}


/// Museum / excavation number from the header.
/// Museum / excavation inventory number.
///
/// Searched in the header first, then in the wider window, because TLHdig keeps
/// it outside the header: `<AO:InvNr>` sits in `<AO:Manuscripts>` in the body.
/// Looking only at the header therefore found it in no document at all, and
/// every one of the 24 000 records carried the missing-value dash — while the
/// numbers are what makes a manuscript findable by its museum id in search.
fn extract_inv(header: &str, window: &str) -> String {
    for hay in [header, window] {
        for tag in [b"InvNr".as_slice(), b"invNr", b"inv"] {
            if let Some(v) = first_tag_text(hay, tag) {
                let t = normalize_ws(&v);
                if !t.is_empty() {
                    return t;
                }
            }
        }
    }
    MISSING.to_string()
}

/// How much of the body to sample when deciding the dominant language.
const LANG_WINDOW: usize = 12 * 1024;

/// Dominant `lg="…"` code among the line elements in the early body window.
fn extract_lang(xml: &str) -> String {
    let window = truncate_on_char_boundary(xml, LANG_WINDOW);
    let mut counts: HashMap<&str, u32> = HashMap::new();

    for (at, _) in window.match_indices("lg=") {
        // Require a token boundary, otherwise `flg="…"` counts as `lg="…"`.
        if window[..at].ends_with(is_name_char) {
            continue;
        }
        let after = &window[at + 3..];
        let Some(quote) = after.chars().next().filter(|&c| c == '"' || c == '\'') else {
            continue;
        };
        let value = &after[quote.len_utf8()..];
        let Some(end) = value.find(quote) else {
            continue;
        };
        let code = &value[..end];
        // Language codes are short; anything longer is some other attribute.
        if !code.is_empty() && code.len() <= 8 {
            *counts.entry(code).or_default() += 1;
        }
    }

    // Ties break on the code itself, so the output does not depend on hash order.
    let winner = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)));
    match winner {
        Some((code, _)) => normalise_lang(code),
        None => MISSING.to_string(),
    }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')
}

/// Fold the spelling variants used across TLHdig into one code per language.
fn normalise_lang(code: &str) -> String {
    match code {
        "Hit" | "Hitt" => "Hit".to_string(),
        "Hur" | "Hurr" => "Hur".to_string(),
        "Akk" | "Akkd" => "Akk".to_string(),
        "ign" | "" => MISSING.to_string(),
        other => other.to_string(),
    }
}

/// Series token from a path segment like `CTH 786_XML_HFR/…`.
fn extract_corpus(path: &str) -> String {
    let mut from = 0;
    while let Some(rel) = find_ci(&path.as_bytes()[from..], b"_XML_") {
        let start = from + rel + 5;
        let corpus = path
            .get(start..)
            .and_then(|rest| rest.split(['/', '\\']).next())
            .unwrap_or("")
            .trim();
        if !corpus.is_empty() {
            return corpus.to_string();
        }
        from = start;
    }
    MISSING.to_string()
}

/// Returns true if the archive path looks like an XML manuscript we should index.
pub fn is_manuscript_xml(path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() < 4 {
        return false;
    }
    let ext = &bytes[bytes.len() - 4..];
    if !ext.eq_ignore_ascii_case(b".xml") {
        return false;
    }
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if name.starts_with('.') || name.starts_with("._") {
        return false;
    }
    let nb = name.as_bytes();
    if nb.len() >= 8 && nb[nb.len() - 8..].eq_ignore_ascii_case(b".css.xml") {
        return false;
    }
    // Optional: skip pure directories (zip stores trailing slash)
    if bytes.last() == Some(&b'/') {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
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

    const SAMPLE_FULL: &str = r#"<?xml-stylesheet href="HPMxml.css" type="text/css"?>
<AOxml xmlns:AO="http://hethiter.net/ns/AO/1.0">
<AOHeader>
  <docID>KBo 17.86+</docID>
  <meta>
    <creation-date date="2016-04-15T16:55:36.58"/>
    <kor2 date="2021-04-22T09:07:54"/>
    <annotation>
      <annot editor="auto" date=""/>
      <annot editor="" date=""/>
    </annotation>
    <neu>
      <uebern editor="FB" date="2017-03-28" src="MZ"/>
      <kor1kf editor="FB" date="2017-06-02"/>
      <kor editor="SG" date="2020-05-27"/>
      <annot editor="UG" date="2021-04-26"/>
    </neu>
  </meta>
</AOHeader>
<body>
  <AO:Manuscripts><AO:TxtPubl>KBo 17.86 {€1}+KBo 15.62 {€2}</AO:TxtPubl></AO:Manuscripts>
</body>
</AOxml>"#;

    #[test]
    fn parses_full_aoxml_with_uebern() {
        let r = parse_manuscript(
            "TLH/CTH 786_XML_HFR/KBo 17.86+.xml",
            SAMPLE_FULL,
        );
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

    #[test]
    fn language_is_the_most_frequent_code() {
        let xml = r#"<body><l lg="Hit"/><l lg="Hit"/><l lg="Akk"/></body>"#;
        assert_eq!(extract_lang(xml), "Hit");
    }

    #[test]
    fn language_accepts_single_quotes_and_folds_variants() {
        assert_eq!(extract_lang("<l lg='Hitt'/>"), "Hit");
        assert_eq!(extract_lang(r#"<l lg="Akkd"/>"#), "Akk");
        assert_eq!(extract_lang(r#"<l lg="ign"/>"#), MISSING);
        assert_eq!(extract_lang("<body/>"), MISSING);
    }

    /// `lg=` must be a whole attribute name, not the tail of another one.
    #[test]
    fn language_ignores_attributes_merely_ending_in_lg() {
        assert_eq!(extract_lang(r#"<l flg="Hit"/>"#), MISSING);
        assert_eq!(extract_lang(r#"<l flg="Hit"/><l lg="Akk"/>"#), "Akk");
    }

    #[test]
    fn corpus_comes_from_the_xml_path_segment() {
        assert_eq!(extract_corpus("TLH/CTH 786_XML_HFR/KBo 17.86+.xml"), "HFR");
        assert_eq!(extract_corpus("a/b_xml_TLH/c.xml"), "TLH");
        assert_eq!(extract_corpus("no/marker/here.xml"), MISSING);
    }

    #[test]
    fn is_manuscript_xml_filters() {
        assert!(is_manuscript_xml("a/b/KBo 1.xml"));
        assert!(!is_manuscript_xml("a/b/readme.txt"));
        assert!(!is_manuscript_xml("a/b/._KBo 1.xml"));
        assert!(!is_manuscript_xml("a/b/.hidden.xml"));
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
