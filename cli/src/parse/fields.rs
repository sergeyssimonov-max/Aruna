//! One function per column of [`ManuscriptRecord`].
//!
//! Each `extract_*` here is given the windows [`super::parse_manuscript`] cut
//! and answers for its own field only: where to look, in what order, and what
//! counts as an answer. The fallbacks are the interesting part — TLHdig is a
//! corpus of hand-edited documents from several decades, and no field sits in
//! the same place in all 24 000 of them.
//!
//! [`ManuscriptRecord`]: super::ManuscriptRecord

use super::{truncate_on_char_boundary, MISSING};
use crate::xml_scan::{
    attr_value, eq_ci, find_ci, find_cth_number, find_year, for_each_start_tag, strip_tags_bytes,
    tag_text,
};
use std::collections::HashMap;
use std::path::Path;

/// Read bytes as UTF-8, or as nothing.
///
/// Every caller here is reading a tag or attribute that the corpus writes in
/// ASCII, and a document that manages invalid UTF-8 in one of them has no
/// answer to give for that field. Empty is that answer.
#[inline]
fn utf8(b: &[u8]) -> &str {
    std::str::from_utf8(b).unwrap_or("")
}

/// Publication siglum: `docID`, else `TxtPubl`, else `title`, else the filename.
///
/// The filename is the last resort rather than the first: it is always present,
/// so consulting it earlier would mask the documents that do carry a siglum.
pub(super) fn extract_sigla(header: &str, xml: &str, path: &str) -> String {
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

/// Catalogue number: the path first, then `<cth>` attributes, then loose text.
///
/// The path wins because the archive is laid out by catalogue number
/// (`CTH 786_XML_HFR/…`), so it is the one statement of the number that is
/// there for every document.
pub(super) fn extract_cth(header: &str, xml: &str, path: &str) -> Option<String> {
    if let Some(n) = find_cth_number(path.as_bytes()) {
        return Some(format!("CTH {}", utf8(n)));
    }
    // <cth neu="CTH 786" …>
    let mut found: Option<String> = None;
    for_each_start_tag(header.as_bytes(), |local, attrs| {
        if !eq_ci(local, b"cth") {
            return false;
        }
        for key in [b"neu".as_slice(), b"alt", b"n"] {
            if let Some(v) = attr_value(attrs, key) {
                if let Some(num) = find_cth_number(v) {
                    found = Some(format!("CTH {}", utf8(num)));
                    return true;
                }
                // value may be bare number
                if !v.is_empty() && v[0].is_ascii_digit() {
                    found = Some(format!("CTH {}", utf8(v)));
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
        return Some(format!("CTH {}", utf8(n)));
    }
    let early = xml.as_bytes().get(..4096.min(xml.len())).unwrap_or(b"");
    find_cth_number(early).map(|n| format!("CTH {}", utf8(n)))
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
    b"author",
    b"val",
    b"format",
    b"kolon",
    b"cth",
];

/// Attributes that name the person, in the order they are believed.
///
/// `editor=` is what most of the corpus uses. `author=` is the newer TLH
/// files' spelling — `<author date="2026-05-11" author="Daniel Schwemer"/>` —
/// and reading only the first name left 549 manuscripts crediting nobody while
/// the document named someone all along.
///
/// `src=` is deliberately not here, though it sits on 27% of documents and
/// looks like initials. It appears only on `<uebern>`, beside that element's
/// own `editor=`, and holds where the transliteration was taken over *from*
/// (`MZ`, `JL`, `DBH`) — the provenance of the takeover, not who made it.
const EDITOR_ATTRS: &[&[u8]] = &[b"editor", b"author"];

/// Editor and, when the same element carries it, the year of that edition.
///
/// The two travel together because they answer one question — who last worked
/// on this document, and when — and taking the year from a different element
/// than the editor would credit one person's work to another's date. The year
/// is optional here; [`extract_year_fallback`] takes over when the winning
/// element carries no date.
pub(super) fn extract_editor_and_year(header: &str) -> (Option<String>, Option<String>) {
    // TEI-like name / author elements first (full names).
    for tag in [b"name".as_slice(), b"persName", b"author"] {
        if let Some(v) = first_tag_text(header, tag) {
            let n = normalize_ws(&v);
            if !n.is_empty() && !is_auto_editor(&n) {
                let year = find_year(header.as_bytes()).map(|y| utf8(&y).to_string());
                return (Some(n), year);
            }
        }
    }

    let mut best: Option<(usize, String, Option<String>)> = None;

    for_each_start_tag(header.as_bytes(), |local, attrs| {
        let Some(ed_raw) = EDITOR_ATTRS.iter().find_map(|key| attr_value(attrs, key)) else {
            return false;
        };
        let editor = normalize_ws(utf8(ed_raw));
        if editor.is_empty() || is_auto_editor(&editor) {
            return false;
        }
        let year = attr_value(attrs, b"date").and_then(|d| year_from_date_value(utf8(d)));
        let prio = EDITOR_ROLE_PRIORITY
            .iter()
            .position(|r| eq_ci(local, r))
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

/// Names that mean "no person edited this".
fn is_auto_editor(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l == "auto" || l == "system" || l == "hfr"
}

/// Year when the editor's own element carried no usable date.
pub(super) fn extract_year_fallback(header: &str, xml: &str) -> Option<String> {
    // Prefer date= on known meta tags.
    let mut year: Option<String> = None;
    for_each_start_tag(header.as_bytes(), |local, attrs| {
        let interesting = eq_ci(local, b"creation-date")
            || eq_ci(local, b"AOxml-creation")
            || eq_ci(local, b"kor2")
            || eq_ci(local, b"kor1")
            || eq_ci(local, b"date");
        if !interesting {
            return false;
        }
        if let Some(d) = attr_value(attrs, b"date").and_then(|d| year_from_date_value(utf8(d))) {
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
        if let Some(y) = year_from_date_value(&t)
            .or_else(|| find_year(t.as_bytes()).map(|y| utf8(&y).to_string()))
        {
            return Some(y);
        }
    }
    find_year(header.as_bytes())
        .or_else(|| find_year(xml.as_bytes().get(..2048.min(xml.len())).unwrap_or(b"")))
        .map(|y| utf8(&y).to_string())
}

/// Leading year of an ISO-ish date value, if it is a plausible edition year.
fn year_from_date_value(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.len() >= 4 && t.as_bytes()[0].is_ascii_digit() {
        let y = &t.as_bytes()[..4];
        if y.iter().all(|c| c.is_ascii_digit()) {
            let n: u32 = utf8(y).parse().ok()?;
            if (1900..=2100).contains(&n) {
                return Some(utf8(y).to_string());
            }
        }
    }
    None
}

/// Museum / excavation inventory number.
///
/// Searched in the header first, then in the wider window, because TLHdig keeps
/// it outside the header: `<AO:InvNr>` sits in `<AO:Manuscripts>` in the body.
/// Looking only at the header therefore found it in no document at all, and
/// every one of the 24 000 records carried the missing-value dash — while the
/// numbers are what makes a manuscript findable by its museum id in search.
pub(super) fn extract_inv(header: &str, window: &str) -> String {
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
pub(super) fn extract_lang(xml: &str) -> String {
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
pub(super) fn extract_corpus(path: &str) -> String {
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

/// Text of the first `<local>` element, tags stripped, or `None` if it is blank.
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

/// Collapse every run of whitespace to one space — the corpus indents freely.
fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
