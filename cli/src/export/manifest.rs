//! The package described for a program rather than a person.
//!
//! The inventory is for reading; this is for the next stage of the project,
//! which turns the manuscripts into PDF. That converter needs the groups and
//! their order to build bookmarks, the per-document metadata to build a table
//! of contents, the output path of every file — and the path each will take
//! when it becomes a PDF — and it needs to know which fonts the corpus demands
//! before it opens a single document.
//!
//! All of that could in principle be scraped back out of the inventory. It
//! should not have to be: a converter that parses HTML to find its inputs is a
//! converter that breaks when the page is restyled. So both documents are
//! written from the same model in one pass, and neither is derived from the
//! other.
//!
//! **The schema below is this exporter's own.** The plan that asked for a
//! manifest was cut off before it said what should be in one, so the shape is
//! chosen from what the stated purpose needs. It is stated here rather than
//! left implicit, because it is a contract and someone will write against it.
//!
//! Written by hand, like `catalog.rs`, for the same reason: this program
//! carries no serialisation dependency, and one field order written out is
//! cheaper than one more crate.

use super::naming::{href, pdf_path};
use super::verify::{self, ADD_DECLARATION, DROP_BOM, REFLOW_PROLOGUE};
use super::{Placed, PACKAGE};
use crate::parse::{group_runs, ManuscriptRecord};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

/// Version of the manifest's own shape.
///
/// A reader that does not recognise it should stop rather than guess. Bumped
/// when a field changes meaning or leaves; adding a field does not bump it.
pub const SCHEMA: u32 = 1;

/// What the corpus asks of a typesetter, counted from the documents themselves.
///
/// Not a list of fonts — the package ships none, and choosing them belongs to
/// whoever renders. It is the coverage that must be satisfied: which Unicode
/// blocks the text actually uses, and in how many documents. A converter that
/// picks a font without knowing the corpus contains 23 000 documents of
/// cuneiform finds out at the end.
#[derive(Default, Clone)]
pub struct FontContract {
    /// Block name → how many documents contain at least one of its code points.
    pub blocks: BTreeMap<&'static str, usize>,
    /// Documents whose text is not in Unicode NFC.
    ///
    /// Recorded rather than corrected: the corpus mixes forms, and a renderer
    /// that assumes composed diacritics will place marks wrongly on the rest.
    pub not_nfc: usize,
    /// Documents containing private-use code points.
    ///
    /// Called out separately from the blocks because it is not a coverage
    /// question a renderer can answer by choosing a better font: these code
    /// points carry whatever meaning the corpus assigned them, and no font
    /// outside the project knows it. A converter that ignores this line ships
    /// PDFs with empty boxes in them.
    pub private_use: usize,
    /// Documents examined.
    pub documents: usize,
    /// Every private-use code point the corpus actually uses.
    ///
    /// The count above says how many documents carry one; this says *which*,
    /// and that is the difference between knowing there is a problem and being
    /// able to act on it. No font outside the project knows what these mean, so
    /// a renderer needs the list to find out whether it can draw them — and on
    /// the machine this was written on, five of the six are drawn by nothing
    /// installed, including the Hittitology fonts.
    ///
    /// A set rather than a count because it is small by nature: six in this
    /// corpus. If a later edition made it large, that is itself the finding.
    pub private_use_points: BTreeSet<u32>,
    /// Characters that are not text and are easy to introduce by accident.
    pub anomalies: Anomalies,
}

/// Code points whose presence is a question rather than a fact.
///
/// None of these is removed — [`crate::export::verify`] permits nothing of the
/// kind, and a transliteration may use a non-breaking space or a zero-width
/// joiner on purpose. They are *counted*, so that a corpus which suddenly grew
/// a thousand replacement characters says so instead of shipping them.
///
/// Counted by document, like everything else here: one document with four
/// hundred soft hyphens is one document with a soft-hyphen habit, not four
/// hundred findings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Anomalies {
    /// U+00A0 and the other spaces that are not U+0020.
    pub unusual_space: usize,
    /// U+200B–U+200D and U+FEFF away from the start of a document.
    pub zero_width: usize,
    /// U+00AD, which is invisible until a renderer decides to break a line.
    pub soft_hyphen: usize,
    /// C0 other than tab, newline and carriage return, and the whole of C1.
    pub control: usize,
    /// The marks that reorder text: a PDF that ignores them lays it out wrongly.
    pub bidi_control: usize,
    /// U+FFFD — decoding went wrong somewhere upstream, or the source is broken.
    pub replacement: usize,
}

impl Anomalies {
    /// Whether anything at all was found.
    pub fn any(&self) -> bool {
        self.unusual_space
            + self.zero_width
            + self.soft_hyphen
            + self.control
            + self.bidi_control
            + self.replacement
            > 0
    }

    /// Each class with its document count, in a fixed order.
    pub fn counts(&self) -> [(&'static str, usize); 6] {
        [
            ("unusual_space", self.unusual_space),
            ("zero_width", self.zero_width),
            ("soft_hyphen", self.soft_hyphen),
            ("control", self.control),
            ("bidi_control", self.bidi_control),
            ("replacement", self.replacement),
        ]
    }
}

/// Which anomaly class `cp` belongs to, if any.
///
/// Asked only of characters outside plain ASCII text, so the corpus's four
/// hundred million Basic Latin characters never reach the comparisons below.
fn anomaly_of(cp: u32) -> Option<usize> {
    Some(match cp {
        // C0 without the three that are ordinary whitespace, and C1 whole.
        0x00..=0x08 | 0x0B | 0x0C | 0x0E..=0x1F | 0x7F..=0x9F => 3,
        0x00A0 | 0x1680 | 0x2000..=0x200A | 0x202F | 0x205F | 0x3000 => 0,
        0x00AD => 2,
        0x200B..=0x200D | 0xFEFF => 1,
        0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069 => 4,
        0xFFFD => 5,
        _ => return None,
    })
}

/// The Unicode blocks this corpus actually uses, listed most-frequent first so
/// the scan below settles early. Anything outside them is counted under
/// `Other`; measured against the whole corpus, `Other` is empty, and if a
/// later edition makes it non-zero that is a signal to come back here.
///
/// The list is not a guess at what Hittite transliteration might need. It is
/// what these 23 936 documents contain, counted. Six blocks would have covered
/// the letters and the cuneiform and left everything else — arrows, shaded
/// blocks, circled letters, private-use code points — piled under one
/// meaningless `Other` in almost every document, which tells a typesetter
/// nothing about which of them it must be able to draw.
const BLOCKS: [(&str, u32, u32); 24] = [
    ("Basic Latin", 0x0000, 0x007F),
    ("Latin-1 Supplement", 0x0080, 0x00FF),
    ("Latin Extended-A", 0x0100, 0x017F),
    ("General Punctuation", 0x2000, 0x206F),
    ("Block Elements", 0x2580, 0x259F),
    ("Arrows", 0x2190, 0x21FF),
    ("Latin Extended Additional", 0x1E00, 0x1EFF),
    ("Superscripts and Subscripts", 0x2070, 0x209F),
    ("Enclosed Alphanumerics", 0x2460, 0x24FF),
    ("Cuneiform", 0x12000, 0x1254F),
    ("Combining Diacritical Marks", 0x0300, 0x036F), // COMBINING
    ("Spacing Modifier Letters", 0x02B0, 0x02FF),
    ("Miscellaneous Technical", 0x2300, 0x23FF),
    ("Currency Symbols", 0x20A0, 0x20CF),
    ("Supplementary Private Use Area-B", 0x100000, 0x10FFFD),
    ("Mathematical Operators", 0x2200, 0x22FF),
    ("Supplemental Punctuation", 0x2E00, 0x2E7F),
    ("Miscellaneous Mathematical Symbols-A", 0x27C0, 0x27EF),
    ("Control Pictures", 0x2400, 0x243F),
    ("Number Forms", 0x2150, 0x218F),
    ("Greek and Coptic", 0x0370, 0x03FF),
    // One SOF PASUQ, in one document, almost certainly a slip for a colon in
    // the source. Named anyway: the source is not ours to correct, and a
    // renderer that has not been told will draw an empty box there.
    ("Hebrew", 0x0590, 0x05FF),
    ("Private Use Area", 0xE000, 0xF8FF),
    ("Supplementary Private Use Area-A", 0xF0000, 0xFFFFD),
];

/// Where [`BLOCKS`] holds the combining marks.
///
/// Pinned by name in the block-table test rather than trusted: the NFC count
/// rides on this index, and a block inserted above it would move it silently.
const COMBINING: usize = 10;

/// The blocks above whose code points no general-purpose font can be expected
/// to draw, because they mean whatever the corpus decided they mean.
///
/// Named rather than repeated as ranges, so each range has one definition, and
/// counted from the blocks already matched rather than in a second pass: the
/// character loop below runs over some four hundred million code points, and
/// three more comparisons on each of them buy nothing the block index does not
/// already say.
const PRIVATE_USE: [&str; 3] = [
    "Private Use Area",
    "Supplementary Private Use Area-A",
    "Supplementary Private Use Area-B",
];

/// The same three areas as code-point ranges, for collecting which of them the
/// corpus actually uses.
///
/// The block index above answers "does this document use one?"; a renderer
/// needs "which ones does the corpus use?", and that is a question about code
/// points rather than about blocks. Cheap to ask because it is only asked of
/// characters outside printable ASCII.
const PRIVATE_USE_RANGES: [(u32, u32); 3] = [
    (0xE000, 0xF8FF),
    (0xF_0000, 0xF_FFFD),
    (0x10_0000, 0x10_FFFD),
];

impl FontContract {
    /// Fold one document's text into the contract.
    pub fn observe(&mut self, text: &str) {
        self.documents += 1;
        let mut seen = [false; BLOCKS.len()];
        let mut other = false;
        let mut private = false;
        let mut odd = [false; 6];

        // By code point, never by UTF-16 unit: cuneiform lives above the BMP,
        // and splitting a surrogate pair would corrupt the count as surely as
        // it would corrupt the text.
        for ch in text.chars() {
            let cp = ch as u32;
            // The corpus is four hundred million characters and most of them
            // are ordinary printable ASCII; those cannot be an anomaly and are
            // let past before anything else is asked.
            if !(0x20..0x7F).contains(&cp) {
                if let Some(class) = anomaly_of(cp) {
                    odd[class] = true;
                }
                if PRIVATE_USE_RANGES
                    .iter()
                    .any(|(lo, hi)| cp >= *lo && cp <= *hi)
                {
                    self.private_use_points.insert(cp);
                }
            }
            match BLOCKS.iter().position(|(_, lo, hi)| cp >= *lo && cp <= *hi) {
                Some(i) => seen[i] = true,
                None => other = true,
            }
        }
        let counters = [
            &mut self.anomalies.unusual_space,
            &mut self.anomalies.zero_width,
            &mut self.anomalies.soft_hyphen,
            &mut self.anomalies.control,
            &mut self.anomalies.bidi_control,
            &mut self.anomalies.replacement,
        ];
        for (present, counter) in odd.iter().zip(counters) {
            if *present {
                *counter += 1;
            }
        }
        for (i, present) in seen.iter().enumerate() {
            if *present {
                let name = BLOCKS[i].0;
                *self.blocks.entry(name).or_default() += 1;
                private |= PRIVATE_USE.contains(&name);
            }
        }
        if other {
            *self.blocks.entry("Other").or_default() += 1;
        }
        if private {
            self.private_use += 1;
        }
        // Not a second pass. "Is this text composed?" asks whether it contains
        // a combining mark, and the loop above has just answered exactly that
        // for the whole document — the block table has no overlaps, so a code
        // point in that range matches this block and nothing else.
        //
        // Asking again cost a full character-by-character walk of every
        // document. Measured on the corpus with `bench_fonts`: 561 ms before,
        // 307 ms after, for the identical counts.
        if seen[COMBINING] {
            self.not_nfc += 1;
        }
    }
}

/// Write the package manifest.
///
/// `records` and `placed` are the same two slices the inventory is written
/// from, in the same order, so the two documents cannot describe different
/// packages.
pub fn render_manifest(
    records: &[ManuscriptRecord],
    placed: &[Placed],
    source: &str,
    archive_md5: &str,
    normalisation: &BTreeMap<String, usize>,
    fonts: &FontContract,
) -> String {
    // The real manifest averages about 350 bytes per document; 1 MiB for an
    // 8.3 MB result meant four reallocations and settling at 16 MiB.
    let mut out = String::with_capacity(placed.len() * 384 + 8192);
    let groups = group_runs(records).count();

    out.push_str("{\n");
    let _ = writeln!(out, "  \"schema\": {SCHEMA},");
    let _ = writeln!(out, "  \"package\": {},", string(PACKAGE));
    let _ = writeln!(
        out,
        "  \"inventory\": {},",
        string(&format!("{PACKAGE}.html"))
    );

    let _ = writeln!(out, "  \"source\": {{");
    let _ = writeln!(out, "    \"label\": {},", string(source));
    let _ = writeln!(out, "    \"archive_md5\": {}", string(archive_md5));
    out.push_str("  },\n");

    let _ = writeln!(out, "  \"counts\": {{");
    let _ = writeln!(out, "    \"groups\": {groups},");
    let _ = writeln!(out, "    \"documents\": {}", placed.len());
    out.push_str("  },\n");

    // What the normaliser was permitted to do, and what it actually did. A
    // converter reading a document can tell an added declaration from one the
    // corpus wrote.
    out.push_str("  \"normalisation\": {\n");
    out.push_str("    \"permitted\": [\n");
    let permitted = permitted();
    for (i, rule) in permitted.iter().enumerate() {
        let _ = writeln!(out, "      {}{}", string(rule), comma(i, permitted.len()));
    }
    out.push_str("    ],\n");
    out.push_str("    \"applied\": {\n");
    for (i, (rule, count)) in normalisation.iter().enumerate() {
        let _ = writeln!(
            out,
            "      {}: {count}{}",
            string(rule),
            comma(i, normalisation.len())
        );
    }
    out.push_str("    }\n  },\n");

    // The font contract: coverage the corpus demands, not fonts it ships.
    out.push_str("  \"fonts\": {\n");
    out.push_str("    \"files_included\": false,\n");
    let _ = writeln!(
        out,
        "    \"note\": {},",
        string(
            "Coverage required by the corpus, counted from the documents. \
             The package ships no font files; choosing them belongs to the renderer."
        )
    );
    let _ = writeln!(out, "    \"documents_examined\": {},", fonts.documents);
    let _ = writeln!(out, "    \"documents_not_in_nfc\": {},", fonts.not_nfc);
    let _ = writeln!(
        out,
        "    \"documents_with_private_use\": {},",
        fonts.private_use
    );
    // Which private-use code points, not just how many documents have one. A
    // renderer cannot look for a font that draws "some private-use character".
    out.push_str("    \"private_use_points\": [");
    for (i, cp) in fonts.private_use_points.iter().enumerate() {
        let _ = write!(out, "{}\"U+{cp:04X}\"", if i == 0 { "" } else { ", " });
    }
    out.push_str("],\n");
    // Characters that are not text. Reported rather than removed: a
    // transliteration may use a non-breaking space on purpose, and this
    // package changes nothing about what a document says.
    out.push_str("    \"anomalies\": {\n");
    let anomalies = fonts.anomalies.counts();
    for (i, (name, count)) in anomalies.iter().enumerate() {
        let _ = writeln!(
            out,
            "      {}: {count}{}",
            string(name),
            comma(i, anomalies.len())
        );
    }
    out.push_str("    },\n");
    out.push_str("    \"blocks\": {\n");
    for (i, (block, count)) in fonts.blocks.iter().enumerate() {
        let _ = writeln!(
            out,
            "      {}: {count}{}",
            string(block),
            comma(i, fonts.blocks.len())
        );
    }
    out.push_str("    }\n  },\n");

    // The groups, in the order the inventory lists them — which is the order a
    // table of contents wants.
    out.push_str("  \"groups\": [\n");
    for (g, (label, run, slice)) in super::group_slices(records, placed).enumerate() {
        let dir = PathBuf::from(super::dir_component(label));

        out.push_str("    {\n");
        let _ = writeln!(out, "      \"label\": {},", string(label));
        let _ = writeln!(out, "      \"dir\": {},", string(&dir.to_string_lossy()));
        let _ = writeln!(out, "      \"documents\": [");

        for (d, (record, place)) in run.iter().zip(slice).enumerate() {
            let _ = writeln!(out, "        {{");
            let _ = writeln!(out, "          \"siglum\": {},", string(&place.label));
            let _ = writeln!(out, "          \"title\": {},", string(&record.title));
            let _ = writeln!(
                out,
                "          \"file\": {},",
                string(&place.relative.to_string_lossy())
            );
            let _ = writeln!(
                out,
                "          \"href\": {},",
                string(&href(&place.relative))
            );
            // Where this document's PDF will go. Named here so the converter
            // does not invent a second naming rule for the same document.
            let _ = writeln!(
                out,
                "          \"pdf\": {},",
                string(&pdf_path(&place.relative).to_string_lossy())
            );
            let _ = writeln!(out, "          \"lang\": {},", string(&record.lang));
            let _ = writeln!(out, "          \"corpus\": {},", string(&record.corpus));
            let _ = writeln!(out, "          \"editor\": {},", string(&record.authorship));
            let _ = writeln!(out, "          \"year\": {},", string(&record.year));
            let _ = writeln!(out, "          \"inv\": {}", string(&record.inv));
            let _ = writeln!(out, "        }}{}", comma(d, run.len()));
        }
        out.push_str("      ]\n");
        let _ = writeln!(out, "    }}{}", comma(g, groups));
    }
    out.push_str("  ]\n}\n");
    out
}

/// The permit list, as the manifest states it.
///
/// Built from [`verify`]'s own table rather than restated here. The two used to
/// be separate lists joined by a shared prefix — `applied` counted a change
/// under `DROP_PI xml-stylesheet` and `permitted` advertised the same string,
/// with nothing relating them — so adding a rule to one left the other quietly
/// describing the old package.
fn permitted() -> Vec<String> {
    let mut out = vec![format!("{DROP_BOM}: a leading U+FEFF")];
    out.extend(
        verify::DROPPED
            .iter()
            .map(|(target, why)| format!("{}: {why}", verify::drop_pi(target))),
    );
    out.push(format!(
        "{ADD_DECLARATION}: {}",
        String::from_utf8_lossy(verify::DECLARATION).trim_end()
    ));
    out.push(format!(
        "{REFLOW_PROLOGUE}: between prologue instructions, to one newline"
    ));
    out
}

/// The separator after item `i` of `len`: a comma, unless it is the last.
///
/// Written out four times in three different shapes before this existed.
fn comma(i: usize, len: usize) -> &'static str {
    if i + 1 == len {
        ""
    } else {
        ","
    }
}

/// A JSON string value, escaped, as a fresh `String`.
///
/// The escaping itself is [`crate::catalog::json_str`] — the crate's one answer
/// to "what does a JSON string look like here", shared with the catalogue the
/// site reads. This wrapper exists because most of the manifest is assembled
/// with `writeln!` and wants a value it can interpolate.
fn string(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    crate::catalog::json_str(raw, &mut out);
    out
}

/// Every string value the manifest gives for `key`, decoded.
///
/// A reader for the one document this program writes, not a JSON parser: it
/// finds `"key": "` and takes the string that follows. It understands every
/// escape JSON defines, which is a superset of the five [`string`] emits —
/// decoding more than the writer produces costs nothing and means the reader
/// does not have to be revisited if the writer ever escapes more.
///
/// It exists so the validator can hold the manifest to the same package the
/// inventory is held to. Two documents written from one model in one pass
/// should not be able to disagree — but "should not" is not a check, and the
/// manifest is what the next stage of this project will read.
pub fn values_of(json: &str, key: &str) -> Vec<String> {
    let needle = format!("\"{key}\": \"");
    let mut out = Vec::new();
    let mut rest = json;
    while let Some(at) = rest.find(&needle) {
        rest = &rest[at + needle.len()..];
        // Almost every value is a path with nothing to unescape. Copying the
        // slice whole beats pushing it one character at a time, and the manifest
        // holds some fifty thousand of them.
        let plain = rest
            .as_bytes()
            .iter()
            .position(|b| matches!(b, b'"' | b'\\'));
        if let Some(end) = plain {
            if rest.as_bytes()[end] == b'"' {
                out.push(rest[..end].to_string());
                rest = &rest[end + 1..];
                continue;
            }
        }

        let mut value = String::new();
        let mut chars = rest.char_indices();
        let mut closed = None;
        while let Some((i, ch)) = chars.next() {
            match ch {
                '"' => {
                    closed = Some(i + 1);
                    break;
                }
                '\\' => match chars.next().map(|(_, c)| c) {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('/') => value.push('/'),
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('b') => value.push('\u{8}'),
                    Some('f') => value.push('\u{c}'),
                    Some('u') => {
                        let hex: String = (0..4)
                            .filter_map(|_| chars.next().map(|(_, c)| c))
                            .collect();
                        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            Some(c) => value.push(c),
                            None => break,
                        }
                    }
                    _ => break,
                },
                c => value.push(c),
            }
        }
        match closed {
            Some(end) => {
                out.push(value);
                rest = &rest[end..];
            }
            // An unterminated string means the rest is not readable either.
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{place, tests_support::fragment};

    fn built() -> (Vec<ManuscriptRecord>, Vec<Placed>) {
        let fragments = vec![
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("Bo 2023/23", "CTH 5", "root/CTH 5_XML_HFR/b.xml"),
            fragment("KUB 2.1", "CTH 9", "root/CTH 9_XML_TLH/c.xml"),
        ];
        let placed = place(&fragments).expect("placed");
        (fragments.into_iter().map(|f| f.record).collect(), placed)
    }

    fn manifest() -> String {
        let (records, placed) = built();
        let mut applied = BTreeMap::new();
        applied.insert("DROP_PI xml-stylesheet".to_string(), 2usize);
        let mut fonts = FontContract::default();
        fonts.observe("KBo 1.1 šarrum 𒀀");
        render_manifest(&records, &placed, "test", "abc123", &applied, &fonts)
    }

    /// The manifest has to be JSON before it has to be anything else.
    #[test]
    fn the_manifest_parses_and_says_what_the_package_holds() {
        let text = manifest();
        // A hand-written writer earns a structural check: balanced braces and
        // brackets, and no trailing comma before a close.
        let braces = text.matches('{').count() as i64 - text.matches('}').count() as i64;
        let brackets = text.matches('[').count() as i64 - text.matches(']').count() as i64;
        assert_eq!(braces, 0, "unbalanced braces");
        assert_eq!(brackets, 0, "unbalanced brackets");
        assert!(!text.contains(",\n  ]"), "trailing comma before a close");
        assert!(!text.contains(",\n    }"), "trailing comma before a close");

        assert!(text.contains("\"schema\": 1"));
        assert!(text.contains("\"groups\": 2"));
        assert!(text.contains("\"documents\": 3"));
    }

    /// What the converter comes for: every document, its file, and the path its
    /// PDF will take.
    #[test]
    fn every_document_carries_its_file_and_its_future_pdf() {
        let text = manifest();
        assert!(text.contains("\"file\": \"CTH 5/KBo 1.1.xml\""));
        assert!(text.contains("\"pdf\": \"CTH 5/KBo 1.1.pdf\""));
        // The siglum with a slash keeps its escaped file name and its own PDF.
        assert!(text.contains("\"siglum\": \"Bo 2023/23\""), "{text}");
        assert!(text.contains("\"file\": \"CTH 5/Bo 2023%2F23.xml\""));
        assert!(text.contains("\"pdf\": \"CTH 5/Bo 2023%2F23.pdf\""));
    }

    /// Non-ASCII is written as itself: a manifest of a cuneiform corpus that
    /// escapes every sign is valid and unreadable.
    #[test]
    fn the_text_is_not_escaped_into_unreadability() {
        let mut fonts = FontContract::default();
        fonts.observe("𒀀 šarrum");
        let (records, placed) = built();
        let text = render_manifest(&records, &placed, "𒀀 source", "x", &BTreeMap::new(), &fonts);
        assert!(text.contains("𒀀 source"));
        assert!(!text.contains("\\u12000"));
    }

    /// The font contract counts what the corpus needs, by code point.
    #[test]
    fn the_font_contract_counts_the_blocks_the_corpus_uses() {
        let mut fonts = FontContract::default();
        fonts.observe("plain ascii");
        fonts.observe("šarrum ḫattuša"); // Latin Extended
        fonts.observe("𒀀𒀁"); // above the BMP
        fonts.observe("e\u{0301}"); // a combining mark: not composed

        assert_eq!(fonts.documents, 4);
        assert_eq!(fonts.blocks.get("Cuneiform"), Some(&1));
        assert_eq!(fonts.blocks.get("Basic Latin"), Some(&3));
        assert_eq!(fonts.blocks.get("Combining Diacritical Marks"), Some(&1));
        assert_eq!(
            fonts.not_nfc, 1,
            "the decomposed one is counted, and only it"
        );
        assert_eq!(fonts.blocks.get("Other"), None, "all of it was classified");
    }

    #[test]
    fn private_use_code_points_are_counted_as_well_as_named() {
        let mut fonts = FontContract::default();
        fonts.observe("ordinary");
        fonts.observe("a\u{100009}b"); // the corpus does use this plane
        fonts.observe("\u{E000}");

        assert_eq!(
            fonts.blocks.get("Supplementary Private Use Area-B"),
            Some(&1)
        );
        assert_eq!(fonts.blocks.get("Private Use Area"), Some(&1));
        assert_eq!(
            fonts.private_use, 2,
            "counted per document, and only where private use appears"
        );
    }

    #[test]
    fn what_the_manifest_writes_can_be_read_back() {
        let awkward = [
            "plain",
            "a\"b",
            "back\\slash",
            "line\nbreak",
            "tab\there",
            "\u{1}",
            "𒀀",
        ];
        let json = format!(
            "{{\n  \"k\": {},\n  \"k\": {},\n  \"k\": {},\n  \"k\": {},\n  \"k\": {},\n  \"k\": {},\n  \"k\": {}\n}}",
            string(awkward[0]),
            string(awkward[1]),
            string(awkward[2]),
            string(awkward[3]),
            string(awkward[4]),
            string(awkward[5]),
            string(awkward[6]),
        );
        assert_eq!(values_of(&json, "k"), awkward);
        assert!(values_of(&json, "absent").is_empty());
    }

    #[test]
    fn every_document_in_the_manifest_can_be_read_back_as_a_path() {
        let (records, placed) = built();
        let json = manifest();
        let files = values_of(&json, "file");
        assert_eq!(files.len(), placed.len(), "one entry per document");
        for (file, place) in files.iter().zip(&placed) {
            assert_eq!(
                std::path::Path::new(file),
                place.relative,
                "the manifest names a path the package does not use"
            );
        }
        assert_eq!(values_of(&json, "pdf").len(), records.len());
    }

    #[test]
    fn the_block_table_holds_together() {
        for (i, (name, lo, hi)) in BLOCKS.iter().enumerate() {
            assert!(lo <= hi, "{name} runs backwards");
            for (other, olo, ohi) in &BLOCKS[i + 1..] {
                assert!(
                    hi < olo || ohi < lo,
                    "{name} and {other} overlap: the first match wins and the \
                     second block would never be counted"
                );
            }
        }
        assert_eq!(
            BLOCKS[COMBINING].0, "Combining Diacritical Marks",
            "COMBINING no longer points at the combining marks, and the NFC \
             count rides on it"
        );
        for name in PRIVATE_USE {
            assert!(
                BLOCKS.iter().any(|(b, _, _)| *b == name),
                "{name} is not a block, so nothing would ever match it and \
                 documents_with_private_use would silently stay zero"
            );
        }
    }

    /// The anomaly classes are recognised by what they are, not by a list of
    /// the ones this corpus happens to contain.
    #[test]
    fn every_anomaly_class_is_recognised() {
        let cases = [
            ('\u{00A0}', "unusual_space"),
            ('\u{2009}', "unusual_space"),
            ('\u{3000}', "unusual_space"),
            ('\u{200B}', "zero_width"),
            ('\u{FEFF}', "zero_width"),
            ('\u{00AD}', "soft_hyphen"),
            ('\u{0001}', "control"),
            ('\u{007F}', "control"),
            ('\u{0085}', "control"),
            ('\u{202E}', "bidi_control"),
            ('\u{2069}', "bidi_control"),
            ('\u{FFFD}', "replacement"),
        ];
        for (ch, class) in cases {
            let mut fonts = FontContract::default();
            fonts.observe(&format!("KBo 1.1{ch}text"));
            let found: Vec<&str> = fonts
                .anomalies
                .counts()
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(name, _)| *name)
                .collect();
            assert_eq!(
                found,
                vec![class],
                "U+{:04X} was classified as {found:?}",
                ch as u32
            );
        }
    }

    /// Ordinary text is not an anomaly, and the three whitespace characters a
    /// document really uses are ordinary.
    #[test]
    fn ordinary_text_raises_nothing() {
        let mut fonts = FontContract::default();
        fonts.observe("KBo 1.1\tCTH 5\nš ḫ ā 𒀀 ①\r\n");
        assert!(
            !fonts.anomalies.any(),
            "ordinary text was reported as anomalous: {:?}",
            fonts.anomalies
        );
    }

    /// Counted once per document, however many times the character occurs.
    #[test]
    fn an_anomaly_is_counted_by_document_and_not_by_character() {
        let mut fonts = FontContract::default();
        fonts.observe(&"\u{00A0}".repeat(400));
        fonts.observe("clean");
        assert_eq!(fonts.anomalies.unusual_space, 1);
        assert_eq!(fonts.documents, 2);
    }

    /// The private-use code points are collected as themselves.
    ///
    /// The count of documents was never enough to act on: a renderer has to
    /// know *which* code points to find a face for, and on the machine this was
    /// written on five of the six this corpus uses are drawn by nothing
    /// installed — including the Hittitology fonts.
    #[test]
    fn the_private_use_code_points_are_collected_and_not_merely_counted() {
        let mut fonts = FontContract::default();
        fonts.observe("a\u{100009}b");
        fonts.observe("c\u{100009}d\u{E000}e");
        assert_eq!(
            fonts.private_use_points.iter().copied().collect::<Vec<_>>(),
            vec![0xE000, 0x10_0009]
        );
        assert_eq!(fonts.private_use, 2, "both documents carry one");
    }

    /// A cuneiform sign is not private use, and a private-use code point is not
    /// a cuneiform sign — the two are next to each other in every discussion of
    /// this corpus and are different problems.
    #[test]
    fn cuneiform_is_not_counted_as_private_use() {
        let mut fonts = FontContract::default();
        fonts.observe("\u{12000}\u{1230B}");
        assert!(fonts.private_use_points.is_empty());
        assert_eq!(fonts.private_use, 0);
        assert_eq!(fonts.blocks.get("Cuneiform"), Some(&1));
    }
}
