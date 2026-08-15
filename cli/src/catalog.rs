//! The site's catalog: the same records the HTML shows, as the JSON wire form.
//!
//! `public/data/inventory.json` is what `scripts/build-inventory-bin.mjs` reads
//! to build the binary the browser downloads, so this module and that script
//! are the two halves of one format:
//!
//! ```json
//! { "s": source, "m": count, "p": [pooled strings],
//!   "g": [[cth label, [[siglum, auth, year, lang, inv, corpus], …]], …],
//!   "v": 2 }
//! ```
//!
//! Metadata fields are indices into the pool `p` — the corpus has 24 000
//! manuscripts and a few dozen distinct editors, years and languages between
//! them. Grouping and order follow the HTML exactly, so the site lists
//! manuscripts in the order the CLI does.
//!
//! It lives in the library rather than in the example that writes the file
//! because a format with no producer is how the catalog drifted from the parser
//! in the first place — and here it can be tested without an archive.

use crate::parse::{group_label, group_runs, ManuscriptRecord};
use std::collections::HashMap;

/// Wire schema version, carried as `v` and checked by the reader.
const WIRE_VERSION: u32 = 2;

/// A rendered catalog: the document, and what went into it.
pub struct Catalog {
    pub json: String,
    /// Distinct strings in the pool — reported by the emitter.
    pub pooled_strings: usize,
}

/// Render the catalog for `records`, attributing it to `source`.
pub fn render(records: &[ManuscriptRecord], source: &str) -> Catalog {
    let mut pool = Pool::default();

    // Groups first: the pool is only complete once every row has been interned.
    let groups = render_groups(records, &mut pool);

    let mut json = String::with_capacity(1 << 20);
    json.push('{');
    json.push_str("\"s\":");
    json_str(source, &mut json);
    json.push_str(&format!(",\"m\":{}", records.len()));
    json.push_str(",\"p\":[");
    for (n, s) in pool.items.iter().enumerate() {
        if n > 0 {
            json.push(',');
        }
        json_str(s, &mut json);
    }
    json.push(']');
    json.push_str(",\"g\":");
    json.push_str(&groups);
    json.push_str(&format!(",\"v\":{WIRE_VERSION}}}"));

    Catalog {
        json,
        pooled_strings: pool.items.len(),
    }
}

/// `[[label, [[siglum, auth, year, lang, inv, corpus], …]], …]`.
fn render_groups(records: &[ManuscriptRecord], pool: &mut Pool) -> String {
    let mut out = String::new();
    out.push('[');
    for (n, run) in group_runs(records).enumerate() {
        if n > 0 {
            out.push(',');
        }
        out.push('[');
        json_str(group_label(&run[0]), &mut out);
        out.push_str(",[");
        for (n, rec) in run.iter().enumerate() {
            if n > 0 {
                out.push(',');
            }
            render_row(rec, pool, &mut out);
        }
        out.push_str("]]");
    }
    out.push(']');
    out
}

/// One manuscript: its siglum, then five pool indices.
fn render_row(rec: &ManuscriptRecord, pool: &mut Pool, out: &mut String) {
    let auth = pool.intern(&rec.authorship);
    let year = pool.intern(&rec.year);
    let lang = pool.intern(&rec.lang);
    let inv = pool.intern(&rec.inv);
    let corpus = pool.intern(&rec.corpus);
    out.push('[');
    json_str(&rec.sigla, out);
    out.push_str(&format!(",{auth},{year},{lang},{inv},{corpus}]"));
}

/// Interner: the pool holds each distinct string once, rows carry indices.
#[derive(Default)]
struct Pool {
    items: Vec<String>,
    index: HashMap<String, usize>,
}

impl Pool {
    fn intern(&mut self, s: &str) -> usize {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.items.len();
        self.items.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }
}

/// Minimal JSON string escaping — enough for this document, which holds only
/// catalogue text.
fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::MISSING;

    fn rec(sigla: &str, cth: Option<&str>, auth: &str, year: &str) -> ManuscriptRecord {
        ManuscriptRecord {
            title: sigla.to_string(),
            sigla: sigla.into(),
            cth: cth.map(|s| s.to_string()),
            cth_num: cth.and_then(crate::parse::parse_cth_num).unwrap_or(u32::MAX),
            authorship: auth.into(),
            year: year.into(),
            lang: "Hit".into(),
            inv: MISSING.into(),
            corpus: "HFR".into(),
        }
    }

    #[test]
    fn groups_are_runs_of_one_label_in_record_order() {
        let records = vec![
            rec("KBo 1", Some("CTH 1"), "A", "2020"),
            rec("KBo 2", Some("CTH 1"), "A", "2020"),
            rec("KUB 9", Some("CTH 547"), "B", "2021"),
        ];
        let json = render(&records, "src").json;
        assert!(json.contains(r#""g":[["CTH 1",[["KBo 1""#), "{json}");
        assert!(json.contains(r#"["CTH 547",[["KUB 9""#), "{json}");
        assert!(json.contains(r#""m":3"#));
        assert!(json.ends_with(r#""v":2}"#));
    }

    /// Repeated metadata is stored once; the rows point at it. This is what
    /// makes the catalog a third of the size it would otherwise be.
    #[test]
    fn identical_metadata_is_pooled_once() {
        let records = vec![
            rec("KBo 1", Some("CTH 1"), "Otten", "2020"),
            rec("KBo 2", Some("CTH 1"), "Otten", "2020"),
        ];
        let catalog = render(&records, "src");
        assert_eq!(
            catalog.json.matches("\"Otten\"").count(),
            1,
            "the editor is written once and referenced twice"
        );
        // Both rows name the same pool entries.
        assert_eq!(catalog.pooled_strings, 5, "editor, year, lang, inv, corpus");
    }

    /// The source line and any siglum reach the file as text, whatever they
    /// contain — this document is written by hand, not by a JSON library.
    #[test]
    fn quotes_backslashes_and_control_characters_are_escaped() {
        let records = vec![rec("A\"B\\C\tD", Some("CTH 1"), "E\nF", "2020")];
        let json = render(&records, "line\u{1}break").json;
        assert!(json.contains(r#""line\u0001break""#), "{json}");
        assert!(json.contains(r#""A\"B\\C\tD""#), "{json}");
        assert!(json.contains(r#""E\nF""#), "{json}");
    }

    #[test]
    fn an_empty_corpus_is_still_a_document() {
        let json = render(&[], "src").json;
        assert_eq!(json, r#"{"s":"src","m":0,"p":[],"g":[],"v":2}"#);
    }

    /// A record with no CTH is grouped under the missing-value dash, the same
    /// label the HTML gives it.
    #[test]
    fn records_without_a_cth_group_under_the_dash() {
        let json = render(&[rec("Loose", None, "A", "2020")], "src").json;
        assert!(json.contains(&format!(r#"["{MISSING}",[["Loose""#)), "{json}");
    }
}
