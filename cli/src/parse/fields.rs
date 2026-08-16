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
    // `docID` and `TxtPubl` are looked for in the wider window as well: TLHdig
    // puts `<AO:TxtPubl>` in the body, beside the inventory number.
    if let Some(text) = text_of(header, b"docID").or_else(|| text_of(xml, b"docID")) {
        return text;
    }
    if let Some(text) = text_of(header, b"TxtPubl").or_else(|| text_of(xml, b"TxtPubl")) {
        // `KBo 17.86 {€1}+KBo 15.62 {€2}` — the join marks belong to the
        // manuscript list, not to the siglum this row is filed under.
        let primary = text.split('{').next().unwrap_or(&text);
        let primary = normalize_ws(primary);
        if !primary.is_empty() {
            return primary;
        }
    }
    if let Some(text) = text_of(header, b"title") {
        return text;
    }
    file_stem(path).unwrap_or_else(|| MISSING.to_string())
}

/// Text of the first `<tag>`, whitespace collapsed, or `None` if it says nothing.
///
/// The three steps went together at every call site — read, normalise, discard
/// if blank — and writing them out each time is what made the fallback chains
/// above hard to see for what they are.
fn text_of(hay: &str, tag: &[u8]) -> Option<String> {
    let text = normalize_ws(&first_tag_text(hay, tag)?);
    (!text.is_empty()).then_some(text)
}

/// The file's own name, without extension or surrounding space.
fn file_stem(path: &str) -> Option<String> {
    let stem = Path::new(path).file_stem()?.to_str()?.trim();
    (!stem.is_empty()).then(|| stem.to_string())
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
    let from_attribute = first_in_start_tags(header, |local, attrs| {
        if !eq_ci(local, b"cth") {
            return None;
        }
        [b"neu".as_slice(), b"alt", b"n"]
            .iter()
            .filter_map(|key| attr_value(attrs, key))
            .find_map(|value| match find_cth_number(value) {
                Some(number) => Some(format!("CTH {}", utf8(number))),
                // The attribute may hold the bare number instead.
                None if value.first().is_some_and(u8::is_ascii_digit) => {
                    Some(format!("CTH {}", utf8(value)))
                }
                None => None,
            })
    });
    if from_attribute.is_some() {
        return from_attribute;
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
    // A TEI-like element naming a person wins outright: it spells out a full
    // name, where the AOxml roles below carry initials.
    for tag in [b"name".as_slice(), b"persName", b"author"] {
        if let Some(name) = text_of(header, tag).filter(|text| !is_auto_editor(text)) {
            return (Some(name), find_year(header.as_bytes()).map(|y| utf8(&y).to_string()));
        }
    }

    // Otherwise the best `editor=`/`author=` attribute in the header, by role.
    let mut best: Option<Credit> = None;
    for_each_start_tag(header.as_bytes(), |local, attrs| {
        if let Some(candidate) = Credit::read(local, attrs) {
            if candidate.beats(best.as_ref()) {
                best = Some(candidate);
            }
        }
        false // every element is considered; the ranking decides
    });

    match best {
        Some(credit) => (Some(credit.editor), credit.year),
        None => (None, None),
    }
}

/// Someone credited on one element of the header, and how strong the claim is.
struct Credit {
    /// Index into [`EDITOR_ROLE_PRIORITY`]; lower is a stronger claim.
    rank: usize,
    editor: String,
    /// The date on the same element, when it carries a usable one.
    year: Option<String>,
}

impl Credit {
    /// Read one element's claim, or `None` if it credits nobody.
    fn read(local: &[u8], attrs: &[u8]) -> Option<Credit> {
        let raw = EDITOR_ATTRS.iter().find_map(|key| attr_value(attrs, key))?;
        let editor = normalize_ws(utf8(raw));
        if editor.is_empty() || is_auto_editor(&editor) {
            return None;
        }
        Some(Credit {
            rank: EDITOR_ROLE_PRIORITY
                .iter()
                .position(|role| eq_ci(local, role))
                .unwrap_or(usize::MAX),
            editor,
            year: attr_value(attrs, b"date").and_then(|date| year_from_date_value(utf8(date))),
        })
    }

    /// Whether this claim should replace `current`.
    ///
    /// A stronger role wins. Between equals the one carrying a date wins,
    /// because the year is taken from the same element as the editor — a date
    /// borrowed from elsewhere would credit one person's work to another's.
    fn beats(&self, current: Option<&Credit>) -> bool {
        match current {
            None => true,
            Some(best) => {
                self.rank < best.rank
                    || (self.rank == best.rank && best.year.is_none() && self.year.is_some())
            }
        }
    }
}

/// Names that mean "no person edited this".
fn is_auto_editor(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l == "auto" || l == "system" || l == "hfr"
}

/// Year when the editor's own element carried no usable date.
pub(super) fn extract_year_fallback(header: &str, xml: &str) -> Option<String> {
    /// Elements whose `date=` says when this edition was made.
    const DATED_ELEMENTS: &[&[u8]] = &[
        b"creation-date",
        b"AOxml-creation",
        b"kor2",
        b"kor1",
        b"date",
    ];

    let dated = first_in_start_tags(header, |local, attrs| {
        DATED_ELEMENTS.iter().any(|tag| eq_ci(local, tag)).then(|| {
            attr_value(attrs, b"date").and_then(|date| year_from_date_value(utf8(date)))
        })?
    });
    if dated.is_some() {
        return dated;
    }

    // TEI writes it as text: `<date>2019</date>`.
    if let Some(text) = first_tag_text(header, b"date") {
        let from_text = year_from_date_value(&text)
            .or_else(|| find_year(text.as_bytes()).map(|y| utf8(&y).to_string()));
        if from_text.is_some() {
            return from_text;
        }
    }

    // Last resort: any plausible year in the header, then in the document's
    // opening bytes.
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
    // Lazily: the first place that answers wins, and the rest is not read.
    [header, window]
        .into_iter()
        .flat_map(|hay| {
            [b"InvNr".as_slice(), b"invNr", b"inv"]
                .into_iter()
                .map(move |tag| (hay, tag))
        })
        .find_map(|(hay, tag)| text_of(hay, tag))
        .unwrap_or_else(|| MISSING.to_string())
}

/// How much of the body to sample when deciding which languages a text is in.
const LANG_WINDOW: usize = 12 * 1024;

/// Separator between language codes, when a manuscript carries more than one.
const LANG_SEPARATOR: &str = ", ";

/// Every `lg="…"` code among the line elements, most-used first.
///
/// A manuscript is not always in one language: 7% of this corpus mixes two or
/// three, which is a fact about the text rather than noise — a Hittite ritual
/// quoting Hurrian incantations is a different object from a Hittite one, and
/// reporting only the dominant code hid that from anyone reading the table.
///
/// Ordered by how much of the sampled window each language holds, so the first
/// code is the one that used to be reported alone. Ties break on the code
/// itself, so the output never depends on hash order.
pub(super) fn extract_lang(xml: &str) -> String {
    let window = truncate_on_char_boundary(xml, LANG_WINDOW);
    let mut counts: HashMap<String, u32> = HashMap::new();

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
        if code.is_empty() || code.len() > 8 {
            continue;
        }
        // Folded before counting, so `Hit` and `Hitt` are one language rather
        // than two entries that would both be listed.
        let folded = normalise_lang(code);
        if folded == MISSING {
            continue;
        }
        *counts.entry(folded).or_default() += 1;
    }

    if counts.is_empty() {
        return MISSING.to_string();
    }

    let mut ranked: Vec<(String, u32)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .map(|(code, _)| code)
        .collect::<Vec<_>>()
        .join(LANG_SEPARATOR)
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')
}

/// Fold the spelling variants used across TLHdig into one code per language.
///
/// Every entry here is a reading of what the corpus wrote, never a guess about
/// what it meant. `Hattian` is the language `Hat` spelled out — the corpus uses
/// both, and listing them side by side would show one language twice in the
/// same row, which is what surfaced when the column began naming all of them.
///
/// Codes left alone on purpose, because folding them would be an inference
/// rather than a reading: `Lu` (one line, in a manuscript from a Luwian volume,
/// so probably `Luw` — but one occurrence is not evidence enough to merge two
/// languages), and `Lin`, which appears on words rather than lines and whose
/// meaning is not recoverable from the data. Both are shown as the corpus
/// writes them.
fn normalise_lang(code: &str) -> String {
    match code {
        "Hit" | "Hitt" => "Hit".to_string(),
        "Hur" | "Hurr" => "Hur".to_string(),
        "Akk" | "Akkd" => "Akk".to_string(),
        "Hat" | "Hattian" => "Hat".to_string(),
        // `ign` is the corpus saying "not identified". `5f_` is not a language
        // at all: it sits where one belongs on 100 lines whose cuneiform is
        // empty or unreadable, an artefact of data entry. Named outright rather
        // than matched by shape, so a real code that is new to us can never be
        // swallowed by the same rule.
        "ign" | "5f_" | "" => MISSING.to_string(),
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

/// First start tag `pick` has an answer for.
///
/// `for_each_start_tag` reports every tag and asks whether to stop, which left
/// each caller carrying a mutable `found` and remembering to return `true`.
/// This says what those callers meant: the first answer, then stop.
fn first_in_start_tags<T>(hay: &str, mut pick: impl FnMut(&[u8], &[u8]) -> Option<T>) -> Option<T> {
    let mut found = None;
    for_each_start_tag(hay.as_bytes(), |local, attrs| {
        found = pick(local, attrs);
        found.is_some()
    });
    found
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

    /// Every language the text uses, and the one it mostly is first.
    #[test]
    fn languages_are_listed_with_the_dominant_one_first() {
        let xml = r#"<body><l lg="Hit"/><l lg="Hit"/><l lg="Akk"/></body>"#;
        assert_eq!(extract_lang(xml), "Hit, Akk");

        // The order follows the text, not the alphabet.
        let hurrian = r#"<body><l lg="Hur"/><l lg="Hur"/><l lg="Hit"/></body>"#;
        assert_eq!(extract_lang(hurrian), "Hur, Hit");
    }

    /// A single-language manuscript reads exactly as it did before — which is
    /// 93% of this corpus.
    #[test]
    fn one_language_is_still_one_word() {
        assert_eq!(extract_lang(r#"<body><l lg="Hit"/><l lg="Hit"/></body>"#), "Hit");
    }

    /// Spelling variants are one language, not two entries side by side.
    ///
    /// The corpus really does write `Hat` and `Hattian` in one manuscript, and
    /// listing both was the first thing this column got wrong.
    #[test]
    fn variants_fold_before_they_are_listed() {
        assert_eq!(extract_lang(r#"<l lg="Hit"/><l lg="Hitt"/>"#), "Hit");
        assert_eq!(extract_lang(r#"<l lg="Akk"/><l lg="Akkd"/><l lg="Hit"/>"#), "Akk, Hit");
        assert_eq!(extract_lang(r#"<l lg="Hat"/><l lg="Hattian"/><l lg="Hit"/>"#), "Hat, Hit");
    }

    /// `5f_` is not a language: it stands where one belongs on lines with no
    /// readable cuneiform. It must not reach the table as though it were.
    #[test]
    fn the_data_entry_artefact_is_not_a_language() {
        assert_eq!(extract_lang(r#"<l lg="5f_"/>"#), MISSING);
        assert_eq!(extract_lang(r#"<l lg="5f_"/><l lg="Hit"/>"#), "Hit");
    }

    /// Codes we cannot resolve are shown as written rather than guessed at.
    #[test]
    fn unresolved_codes_are_passed_through_unchanged() {
        assert_eq!(extract_lang(r#"<l lg="Lu"/>"#), "Lu");
        assert_eq!(extract_lang(r#"<l lg="Lin"/>"#), "Lin");
        assert_eq!(extract_lang(r#"<l lg="Sum"/><l lg="Pal"/>"#), "Pal, Sum");
    }

    /// A tie must not depend on hash order, or the catalog would differ
    /// between runs of the same parser over the same archive.
    #[test]
    fn equal_counts_are_ordered_by_the_code_itself() {
        let xml = r#"<l lg="Luw"/><l lg="Hat"/><l lg="Akk"/>"#;
        assert_eq!(extract_lang(xml), "Akk, Hat, Luw");
        // Same input, opposite document order: same answer.
        let other = r#"<l lg="Akk"/><l lg="Luw"/><l lg="Hat"/>"#;
        assert_eq!(extract_lang(other), "Akk, Hat, Luw");
    }

    #[test]
    fn language_accepts_single_quotes_and_folds_variants() {
        assert_eq!(extract_lang("<l lg='Hitt'/>"), "Hit");
        assert_eq!(extract_lang(r#"<l lg="Akkd"/>"#), "Akk");
        assert_eq!(extract_lang(r#"<l lg="ign"/>"#), MISSING);
        assert_eq!(extract_lang("<body/>"), MISSING);
    }

    /// `ign` means "not identified" and is dropped, but dropping it must not
    /// hide the languages that *are* named beside it.
    #[test]
    fn unidentified_lines_do_not_erase_the_identified_ones() {
        assert_eq!(extract_lang(r#"<l lg="ign"/><l lg="ign"/><l lg="Hit"/>"#), "Hit");
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
