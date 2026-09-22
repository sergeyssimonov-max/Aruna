//! The titles of the Catalog of Hittite Texts, read from a snapshot in the tree.
//!
//! The inventory shows beside each CTH number the title the catalogue gives
//! that number — verbatim, diacritics and punctuation as the catalogue writes
//! them, nothing translated, shortened or reworded. The catalogue is
//! <https://hethport.net/CTH/index.php?lang=EN>, by S. Košak, G.G.W. Müller,
//! S. Görke and Ch.W. Steitler at the Hethitologie-Portal Mainz, licensed CC
//! BY-SA 4.0 (the terms, quoted, are in `resources/cth/CTH-TITLES-TERMS.txt`,
//! which the package carries beside the inventory).
//!
//! **Nothing here touches the network.** The titles are compiled in from
//! `resources/cth/cth-titles.tsv`, a snapshot with the address it came from,
//! the moment it was taken and the SHA-256 of the page it was taken from.
//! Building a package reads the snapshot and nothing else. Refreshing it is a
//! separate command, `cargo run --locked --release -p aruna --example
//! cth_titles`, which fetches the page, parses it with [`parse_catalog_page`],
//! and replaces the snapshot only once the parse has succeeded.
//!
//! **Matching is by identifier and never by guess.** A corpus label is read as
//! `CTH`, a number and whatever index follows it; the prefix and spaces are
//! normalised, the index is kept. What the catalogue says about that
//! identifier is one of six things, and they are kept apart — see [`Match`].

use std::collections::BTreeMap;

/// Where the snapshot was taken from.
pub const SOURCE_URL: &str = "https://hethport.net/CTH/index.php?lang=EN";

/// The snapshot, compiled in.
pub const SNAPSHOT: &str = include_str!("../resources/cth/cth-titles.tsv");

/// The file beside the inventory that says whose titles these are and on what
/// terms they are reproduced.
pub const PACKAGED_TERMS: &str = "CTH-TITLES-TERMS.txt";

/// Its bytes, compiled in so that the package cannot ship without them.
pub const PACKAGED_TERMS_BYTES: &[u8] = include_bytes!("../resources/cth/CTH-TITLES-TERMS.txt");

/// A CTH identifier: the number, and the index after it, if any.
///
/// `CTH 409` is `{409, ""}`; a letter index `CTH 12a` is `{12, "a"}`; a
/// subdivision `CTH 12.1` is `{12, ".1"}`. The index is kept exactly as
/// written, because `12a` and `12.1` are different texts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Id {
    pub number: u32,
    pub index: String,
}

impl Id {
    /// Read a label such as `CTH 409`, `cth409`, `CTH  12.1` or `CTH 12a`.
    ///
    /// `None` for anything that is not a CTH identifier — a label is never
    /// bent into one.
    pub fn parse(label: &str) -> Option<Id> {
        let s = label.trim();
        let s = match s.get(..3) {
            Some(p) if p.eq_ignore_ascii_case("CTH") => s[3..].trim_start(),
            _ => s,
        };
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let number: u32 = s[..digits].parse().ok()?;
        let index = s[digits..].trim();
        let well_formed = index
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '/');
        if !well_formed || index.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        Some(Id {
            number,
            index: index.to_string(),
        })
    }

    /// The identifier with its index dropped: what a subdivision falls back to.
    fn parent(&self) -> Option<Id> {
        (!self.index.is_empty()).then(|| Id {
            number: self.number,
            index: String::new(),
        })
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CTH {}{}", self.number, self.index)
    }
}

/// What the catalogue lists under one identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A title. Plain text, except that superscript — the determinatives, as
    /// in `(<sup>LÚ</sup>AGRIG)` — is kept as `<sup>…</sup>`, the only markup a
    /// title may hold. Without it `LÚ` would run into the word it qualifies.
    Titled(String),
    /// The catalogue reserves the number and assigns no text to it.
    Unassigned,
}

/// What the catalogue says about one corpus label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match<'c> {
    /// The catalogue gives this very identifier this title.
    Exact(&'c str),
    /// The identifier is a subdivision or a letter index the catalogue does
    /// not list, and this is its parent's title — shown as the parent's, never
    /// as though it were confirmed for the subdivision.
    Parent { parent: Id, title: &'c str },
    /// The group has no CTH at all.
    Missing,
    /// The label is a CTH identifier the catalogue does not list, or not a CTH
    /// identifier at all.
    NotFound,
    /// The catalogue lists the identifier more than once, with different
    /// entries. Which one is meant is not ours to pick.
    Ambiguous,
    /// The catalogue lists the number as unassigned.
    Unassigned,
}

impl Match<'_> {
    /// The status as the manifest names it.
    pub fn code(&self) -> &'static str {
        match self {
            Match::Exact(_) => "exact",
            Match::Parent { .. } => "parent",
            Match::Missing => "missing",
            Match::NotFound => "not_found",
            Match::Ambiguous => "ambiguous",
            Match::Unassigned => "unassigned",
        }
    }
}

/// The snapshot, parsed: where it came from, and what it lists.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub source: String,
    pub fetched: String,
    pub sha256: String,
    entries: BTreeMap<Id, Vec<Entry>>,
}

/// A snapshot line the reader refuses, with its line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotError {
    pub line: usize,
    pub reason: &'static str,
}

impl Catalog {
    /// The snapshot compiled into this build.
    ///
    /// A snapshot that does not parse is a defect of the tree, caught by the
    /// test that reads it, so a build that got this far reads it cleanly; an
    /// empty catalogue is what it falls back to rather than a panic.
    pub fn compiled() -> Catalog {
        Catalog::parse(SNAPSHOT).unwrap_or_default()
    }

    /// Read a snapshot: `# key: value` header lines, then one entry per line —
    /// `CTH N<TAB>title<TAB>text` or `CTH N<TAB>unassigned<TAB>`.
    pub fn parse(text: &str) -> Result<Catalog, SnapshotError> {
        let mut catalog = Catalog::default();
        for (n, line) in text.lines().enumerate() {
            let line_no = n + 1;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(header) = line.strip_prefix('#') {
                if let Some((key, value)) = header.split_once(':') {
                    let value = value.trim().to_string();
                    match key.trim() {
                        "source" => catalog.source = value,
                        "fetched" => catalog.fetched = value,
                        "sha256" => catalog.sha256 = value,
                        _ => {}
                    }
                }
                continue;
            }
            let mut fields = line.split('\t');
            let (Some(id), Some(kind), text) = (fields.next(), fields.next(), fields.next()) else {
                return Err(SnapshotError {
                    line: line_no,
                    reason: "expected identifier, kind and text separated by tabs",
                });
            };
            let id = Id::parse(id).ok_or(SnapshotError {
                line: line_no,
                reason: "not a CTH identifier",
            })?;
            let entry = match (kind, text.unwrap_or("")) {
                ("unassigned", "") => Entry::Unassigned,
                ("title", t) if !t.trim().is_empty() => {
                    if !title_markup_is_valid(t) {
                        return Err(SnapshotError {
                            line: line_no,
                            reason: "a title may carry <sup>…</sup> and no other markup",
                        });
                    }
                    Entry::Titled(t.to_string())
                }
                _ => {
                    return Err(SnapshotError {
                        line: line_no,
                        reason: "kind must be `title` with a text or `unassigned` without one",
                    })
                }
            };
            catalog.entries.entry(id).or_default().push(entry);
        }
        Ok(catalog)
    }

    /// How many identifiers the snapshot lists.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the snapshot lists nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What the catalogue says about the identifier alone, without falling back.
    fn entry(&self, id: &Id) -> Option<Result<&Entry, ()>> {
        let entries = self.entries.get(id)?;
        let first = entries.first()?;
        if entries.iter().all(|e| e == first) {
            Some(Ok(first))
        } else {
            Some(Err(()))
        }
    }

    /// What the catalogue says about one corpus label. `None` is a group with
    /// no CTH.
    pub fn lookup(&self, label: Option<&str>) -> Match<'_> {
        let Some(label) = label else {
            return Match::Missing;
        };
        let Some(id) = Id::parse(label) else {
            return Match::NotFound;
        };
        match self.entry(&id) {
            Some(Ok(Entry::Titled(t))) => Match::Exact(t),
            Some(Ok(Entry::Unassigned)) => Match::Unassigned,
            Some(Err(())) => Match::Ambiguous,
            None => match id.parent() {
                Some(parent) => match self.entry(&parent) {
                    Some(Ok(Entry::Titled(t))) => Match::Parent { parent, title: t },
                    Some(Err(())) => Match::Ambiguous,
                    _ => Match::NotFound,
                },
                None => Match::NotFound,
            },
        }
    }
}

/// Whether a title holds no markup but balanced, unnested `<sup>…</sup>`.
fn title_markup_is_valid(title: &str) -> bool {
    let mut open = false;
    let mut rest = title;
    while let Some(at) = rest.find(['<', '>']) {
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("<sup>") {
            if open {
                return false;
            }
            open = true;
            rest = after;
        } else if let Some(after) = tail.strip_prefix("</sup>") {
            if !open {
                return false;
            }
            open = false;
            rest = after;
        } else {
            return false;
        }
    }
    !open
}

/// A title as the inventory writes it: the text escaped, the superscript
/// written by this function rather than copied.
///
/// The group heading renders this with `{@html}`, so what leaves here is
/// markup. The catalogue's text goes through [`crate::html::escape_html`]; the
/// `<sup>` around it is this function's own, the same arrangement as the
/// editor cell in `html.rs`.
pub fn title_html(title: &str) -> String {
    let mut out = String::with_capacity(title.len() + 16);
    let mut rest = title;
    while let Some(open) = rest.find("<sup>") {
        out.push_str(&crate::html::escape_html(&rest[..open]));
        let inner = &rest[open + "<sup>".len()..];
        let close = inner.find("</sup>").unwrap_or(inner.len());
        out.push_str("<sup>");
        out.push_str(&crate::html::escape_html(&inner[..close]));
        out.push_str("</sup>");
        rest = inner.get(close + "</sup>".len()..).unwrap_or("");
    }
    out.push_str(&crate::html::escape_html(rest));
    out
}

/// A title as plain text: the superscript kept as its letters.
pub fn title_text(title: &str) -> String {
    title.replace("<sup>", "").replace("</sup>", "")
}

// ---------------------------------------------------------------------------
// The catalogue page, for the command that refreshes the snapshot
// ---------------------------------------------------------------------------

/// Read the catalogue page into entries, in page order.
///
/// Each entry is a paragraph of class `STD` holding a link to
/// `hetkonk_abfrage.php?c=N` whose text is `CTH N`, then the title, then an
/// empty keyword span. The title is read to the end of the paragraph, its tags
/// dropped except `<sup>`, its entities decoded and its whitespace collapsed; a
/// paragraph whose whole text is `unassigned` is [`Entry::Unassigned`].
/// Refuses a page with no entries, or an entry whose link and text disagree.
pub fn parse_catalog_page(html: &str) -> Result<Vec<(Id, Entry)>, String> {
    const PARA: &str = "<p class=\"STD\">";
    const LINK: &str = "hetkonk_abfrage.php?c=";
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find(PARA) {
        let body = &rest[at + PARA.len()..];
        let end = body.find("</p>").ok_or("a paragraph that never closes")?;
        let para = &body[..end];
        rest = &body[end..];
        let Some(link) = para.find(LINK) else {
            continue;
        };
        let c = &para[link + LINK.len()..];
        let c = &c[..c
            .find(['"', '\'', '&'])
            .ok_or("a catalogue link that never ends")?];
        let anchor_open = para[link..].find('>').ok_or("a link without text")? + link + 1;
        let anchor_close = para[anchor_open..]
            .find("</a>")
            .ok_or("a link that never closes")?
            + anchor_open;
        let label = collapse(&strip_tags(&para[anchor_open..anchor_close], false));
        let id = Id::parse(&label)
            .ok_or_else(|| format!("link text {label:?} is not a CTH identifier"))?;
        if Id::parse(c) != Some(id.clone()) {
            return Err(format!("link c={c} names {label}"));
        }
        let after = &para[anchor_close + "</a>".len()..];
        let after = after.strip_prefix("</span>").unwrap_or(after);
        let title = collapse(&decode_entities(&strip_tags(after, true)));
        let entry = if title == "unassigned" {
            Entry::Unassigned
        } else if title.is_empty() {
            return Err(format!("{label} has no title"));
        } else {
            Entry::Titled(title)
        };
        out.push((id, entry));
    }
    if out.is_empty() {
        return Err("the page lists no catalogue entries".into());
    }
    Ok(out)
}

/// Drop every tag; with `keep_sup`, keep `<sup>` and `</sup>` as written.
fn strip_tags(s: &str, keep_sup: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let tag_end = rest[open..]
            .find('>')
            .map(|e| open + e + 1)
            .unwrap_or(rest.len());
        let tag = &rest[open..tag_end];
        if keep_sup {
            let name = tag
                .trim_start_matches(['<', '/'])
                .split(|c: char| c == '>' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if name.eq_ignore_ascii_case("sup") {
                out.push_str(if tag.starts_with("</") {
                    "</sup>"
                } else {
                    "<sup>"
                });
            }
        }
        rest = &rest[tag_end..];
    }
    out.push_str(rest);
    out
}

/// Decode the entities HTML text may carry: the five named, `&nbsp;`, and
/// numeric references.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let tail = &rest[amp..];
        let semi = tail.find(';').filter(|&i| i <= 10);
        let decoded = semi.and_then(|i| {
            let name = &tail[1..i];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ if name.starts_with("#x") || name.starts_with("#X") => {
                    u32::from_str_radix(&name[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, i + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse runs of whitespace to one space and trim; no-break spaces count.
fn collapse(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .replace("<sup> ", "<sup>")
        .replace(" </sup>", "</sup>")
}

/// A snapshot file's text for these entries.
pub fn render_snapshot(entries: &[(Id, Entry)], fetched: &str, sha256: &str) -> String {
    let mut sorted: Vec<&(Id, Entry)> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::new();
    out.push_str("# Titles of the Catalog of Hittite Texts (CTH), one line per catalogue entry.\n");
    out.push_str("# S. Košak – G.G.W. Müller – S. Görke – Ch.W. Steitler, 2001–2026, Hethitologie-Portal Mainz.\n");
    out.push_str("# Licensed CC BY-SA 4.0, https://creativecommons.org/licenses/by-sa/4.0/ – see CTH-TITLES-TERMS.txt.\n");
    out.push_str("# Written by `cargo run --locked --release -p aruna --example cth_titles`; do not edit by hand.\n");
    out.push_str(&format!("# source: {SOURCE_URL}\n"));
    out.push_str(&format!("# fetched: {fetched}\n"));
    out.push_str(&format!("# sha256: {sha256}\n"));
    out.push_str(&format!("# entries: {}\n", entries.len()));
    for (id, entry) in sorted {
        match entry {
            Entry::Titled(t) => out.push_str(&format!("{id}\ttitle\t{t}\n")),
            Entry::Unassigned => out.push_str(&format!("{id}\tunassigned\t\n")),
        }
    }
    out
}

/// The moment a snapshot is taken, in the form the inventory prints its own.
pub fn now_utc() -> String {
    crate::format_now_utc()
}

/// A moment given in seconds since the epoch, in the same form: for a page
/// read from a file, the moment the file was written rather than the moment it
/// was parsed.
pub fn utc_from_unix(secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = crate::civil_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(lines: &str) -> Catalog {
        Catalog::parse(lines).expect("test snapshot parses")
    }

    #[test]
    fn the_compiled_snapshot_parses_and_names_its_source() {
        let c = Catalog::parse(SNAPSHOT).expect("the snapshot in the tree parses");
        assert!(c.len() > 800, "only {} entries", c.len());
        assert_eq!(c.source, SOURCE_URL);
        assert_eq!(c.sha256.len(), 64);
        assert!(!c.fetched.is_empty());
    }

    #[test]
    fn identifiers_are_normalised_and_their_index_is_kept() {
        let id = |n, i: &str| {
            Some(Id {
                number: n,
                index: i.into(),
            })
        };
        assert_eq!(Id::parse("CTH 409"), id(409, ""));
        assert_eq!(Id::parse("cth409"), id(409, ""));
        assert_eq!(Id::parse("  CTH   409 "), id(409, ""));
        assert_eq!(Id::parse("CTH 12a"), id(12, "a"));
        assert_eq!(Id::parse("CTH 12.1"), id(12, ".1"));
        assert_eq!(Id::parse("CTH 12/2"), id(12, "/2"));
        assert_eq!(Id::parse("—"), None);
        assert_eq!(Id::parse("CTH"), None);
        assert_eq!(Id::parse("CTH 12 (?)"), None);
        assert_eq!(Id::parse("KBo 1.1"), None);
    }

    #[test]
    fn a_short_title_matches_exactly() {
        let c = catalog("CTH 1\ttitle\tProclamation of Anitta\n");
        assert_eq!(
            c.lookup(Some("CTH 1")),
            Match::Exact("Proclamation of Anitta")
        );
    }

    #[test]
    fn a_long_title_with_diacritics_comes_back_verbatim() {
        let t = "Instructions of Tutḫaliya IV to the princes, lords and courtiers (LÚ.MEŠ SAG) – Ḫattuša, Šapinuwa, Ištanuwa";
        let c = catalog(&format!("CTH 255\ttitle\t{t}\n"));
        assert_eq!(c.lookup(Some("CTH 255")), Match::Exact(t));
    }

    #[test]
    fn a_subdivision_listed_by_the_catalogue_is_exact() {
        let c = catalog("CTH 12\ttitle\tParent\nCTH 12.1\ttitle\tChild\n");
        assert_eq!(c.lookup(Some("CTH 12.1")), Match::Exact("Child"));
    }

    #[test]
    fn an_unlisted_subdivision_falls_back_to_its_parent_and_says_so() {
        let c = catalog("CTH 12\ttitle\tThe Anatolian campaigns of Muršili I\n");
        assert_eq!(
            c.lookup(Some("CTH 12.1")),
            Match::Parent {
                parent: Id {
                    number: 12,
                    index: String::new()
                },
                title: "The Anatolian campaigns of Muršili I"
            }
        );
        assert_eq!(c.lookup(Some("CTH 12a")).code(), "parent");
    }

    #[test]
    fn no_cth_is_missing_and_an_unlisted_one_is_not_found() {
        let c = catalog("CTH 1\ttitle\tA\n");
        assert_eq!(c.lookup(None), Match::Missing);
        assert_eq!(c.lookup(Some("CTH 2")), Match::NotFound);
        assert_eq!(c.lookup(Some("not a cth")), Match::NotFound);
        assert_eq!(c.lookup(Some("CTH 2.1")), Match::NotFound);
    }

    #[test]
    fn two_different_entries_for_one_identifier_are_ambiguous() {
        let c = catalog("CTH 5\ttitle\tOne\nCTH 5\ttitle\tTwo\n");
        assert_eq!(c.lookup(Some("CTH 5")), Match::Ambiguous);
        assert_eq!(c.lookup(Some("CTH 5.1")), Match::Ambiguous);
        let same = catalog("CTH 5\ttitle\tOne\nCTH 5\ttitle\tOne\n");
        assert_eq!(same.lookup(Some("CTH 5")), Match::Exact("One"));
    }

    #[test]
    fn an_unassigned_number_is_unassigned() {
        let c = catalog("CTH 15\tunassigned\t\n");
        assert_eq!(c.lookup(Some("CTH 15")), Match::Unassigned);
    }

    #[test]
    fn a_snapshot_with_other_markup_is_refused() {
        for bad in [
            "CTH 1\ttitle\t<i>x</i>\n",
            "CTH 1\ttitle\t<sup>x\n",
            "CTH 1\ttitle\ta > b\n",
            "CTH 1\ttitle\t\n",
            "CTH 1\tunassigned\tsomething\n",
            "CTH x\ttitle\ty\n",
        ] {
            assert!(Catalog::parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn a_title_is_escaped_and_only_its_superscript_is_markup() {
        assert_eq!(
            title_html("Lists of administrators (<sup>LÚ</sup>AGRIG)"),
            "Lists of administrators (<sup>LÚ</sup>AGRIG)"
        );
        assert_eq!(title_html("A < B & \"C\""), "A &lt; B &amp; &quot;C&quot;");
        assert_eq!(title_html("<sup>a<b</sup>"), "<sup>a&lt;b</sup>");
        assert_eq!(title_text("(<sup>LÚ</sup>AGRIG)"), "(LÚAGRIG)");
    }

    const PAGE: &str = r#"<p class="AOH1">I. HISTORICAL TEXTS</p>
<p class="STD"><span class="CTHcatnrZchn"><a href="/hetkonk/hetkonk_abfrage.php?c=1" target='_blank'> CTH 1 </a></span> Proclamation of Anitta <span class="CTHcatstichw"></span></p>
<p class="STD"><span class="CTHcatnrZchn"><a href="/hetkonk/hetkonk_abfrage.php?c=14" target='_blank'> CTH 14 </a></span> <span class="AO--italic">Res gestae</span> of Ḫattušili I <span class="CTHcatstichw"></span></p>
<p class="STD"><span class="CTHcatnrZchn"><a href="/hetkonk/hetkonk_abfrage.php?c=15" target='_blank'> CTH 15 </a></span> <span class="AO--italic">unassigned</span></p>
<p class="STD"><span class="CTHcatnrZchn"><a href="/hetkonk/hetkonk_abfrage.php?c=263" target='_blank'> CTH 263 </a></span> Lists of administrators (<sup><span class="AO--superscr">LÚ</span></sup><sum>AGRIG</sum>) <span class="CTHcatstichw"></span></p>
<p class="STD"></p>
<p class="STD"><span class="CTHcatnrZchn"><a href="/hetkonk/hetkonk_abfrage.php?c=833" target='_blank'> CTH 833 </a></span> Old Assyrian, primarily from <i>kārum Ḫattuš</i> &amp; more</p>"#;

    #[test]
    fn the_catalogue_page_is_read_entry_by_entry() {
        let entries = parse_catalog_page(PAGE).expect("parses");
        let get = |n| {
            entries
                .iter()
                .find(|(id, _)| id.number == n)
                .map(|(_, e)| e.clone())
        };
        assert_eq!(entries.len(), 5);
        assert_eq!(get(1), Some(Entry::Titled("Proclamation of Anitta".into())));
        assert_eq!(
            get(14),
            Some(Entry::Titled("Res gestae of Ḫattušili I".into()))
        );
        assert_eq!(get(15), Some(Entry::Unassigned));
        assert_eq!(
            get(263),
            Some(Entry::Titled(
                "Lists of administrators (<sup>LÚ</sup>AGRIG)".into()
            ))
        );
        assert_eq!(
            get(833),
            Some(Entry::Titled(
                "Old Assyrian, primarily from kārum Ḫattuš & more".into()
            ))
        );
    }

    #[test]
    fn a_page_with_no_entries_or_a_mislabelled_link_is_refused() {
        assert!(parse_catalog_page("<html></html>").is_err());
        let wrong =
            r#"<p class="STD"><a href="/hetkonk/hetkonk_abfrage.php?c=2">CTH 3</a></span> X</p>"#;
        assert!(parse_catalog_page(wrong).is_err());
    }

    #[test]
    fn a_rendered_snapshot_reads_back_as_the_same_entries() {
        let entries = parse_catalog_page(PAGE).expect("parses");
        let text = render_snapshot(&entries, "2026-09-22 00:00:00 UTC", &"0".repeat(64));
        let c = Catalog::parse(&text).expect("reads back");
        assert_eq!(c.len(), 5);
        assert_eq!(
            c.lookup(Some("CTH 263")),
            Match::Exact("Lists of administrators (<sup>LÚ</sup>AGRIG)")
        );
        assert_eq!(c.lookup(Some("CTH 15")), Match::Unassigned);
        assert_eq!(c.fetched, "2026-09-22 00:00:00 UTC");
        assert_eq!(c.source, SOURCE_URL);
    }
}
