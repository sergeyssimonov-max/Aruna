//! Scandinavian-style HTML inventory generation.
//!
//! Rows are grouped by CTH catalogue number (tablet family); fragments of the
//! same text (e.g. all CTH 547) render under one section heading.
//!
//! The document is assembled from three kinds of thing, kept apart on purpose:
//! the fixed chunks below, which are the page as it would be written by hand;
//! [`COLUMNS`], the one description of the table's columns; and the rows, which
//! are the only part that depends on the records.

use crate::parse::{group_label, group_runs, ManuscriptRecord, MISSING};
use std::fmt::Write as _;

/// Escape text for safe HTML text/attribute embedding.
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// One table column: its heading, its `<colgroup>` class, and its explanation.
///
/// The legend, the `<colgroup>` and the `<thead>` are three parallel lists of
/// the same six columns; written out three times, they were three places to
/// forget. Adding or renaming a column is one line here.
struct Column {
    /// Text in the `<th>`.
    head: &'static str,
    /// Class on the `<col>`, which the stylesheet gives its width.
    class: &'static str,
    /// What the legend above the table says the column holds.
    legend: &'static str,
}

const COLUMNS: [Column; 6] = [
    Column {
        head: "№",
        class: "c-num",
        legend: "row number",
    },
    Column {
        head: "Siglum",
        class: "c-sig",
        legend: "publication id (e.g. KBo 3.22)",
    },
    Column {
        head: "Lang",
        class: "c-lang",
        legend: "dominant language (Hit, Hur, Akk…)",
    },
    Column {
        head: "Corpus",
        class: "c-corp",
        legend: "edition series (HFR, TLH, HAnn…)",
    },
    Column {
        head: "Editor",
        class: "c-ed",
        legend: "transliteration / edition author",
    },
    Column {
        head: "Year",
        class: "c-year",
        legend: "edition year",
    },
];

/// Build the full HTML document.
///
/// `source` — human-readable source line (Zenodo record).
/// `generated_at` — already-formatted local date/time string.
pub fn render_html(records: &[ManuscriptRecord], source: &str, generated_at: &str) -> String {
    let (body_rows, groups) = render_rows(records);

    let mut html = String::with_capacity(4096 + body_rows.len());
    html.push_str(DOCUMENT_HEAD);
    html.push_str(include_str!("html_style.css"));
    html.push_str(HEAD_TO_BODY);
    write_summary(&mut html, source, generated_at, records.len(), groups);
    write_legend(&mut html);
    html.push_str(TOOLBAR);
    write_table(&mut html, &body_rows);
    html.push_str(BODY_TO_SCRIPT);
    html.push_str(include_str!("html_filter.js"));
    html.push_str(DOCUMENT_TAIL);
    html
}

/// The table body: one section row per CTH group, then its manuscripts.
///
/// Returns the rows and how many groups they fell into — the count is part of
/// the summary line, and counting it here means walking the records once.
///
/// The records arrive sorted (`archive::sort_records`), so a group is a run of
/// equal labels rather than something to collect into a map.
fn render_rows(records: &[ManuscriptRecord]) -> (String, usize) {
    let mut rows = String::new();
    let mut row_n = 0usize;
    let mut groups = 0usize;

    for run in group_runs(records) {
        groups += 1;
        write_group_row(&mut rows, group_label(&run[0]), run.len());

        for rec in run {
            row_n += 1;
            write_item_row(&mut rows, row_n, rec);
        }
    }

    (rows, groups)
}

fn write_group_row(out: &mut String, label: &str, count: usize) {
    let label = escape_html(label);
    let _ = writeln!(
        out,
        "        <tr class=\"group\">\n          <td colspan=\"6\"><span class=\"group-label\">{label}</span><span class=\"group-count\">{count}</span></td>\n        </tr>"
    );
}

fn write_item_row(out: &mut String, row_n: usize, rec: &ManuscriptRecord) {
    // Within a CTH group show the siglum as the primary name: the group heading
    // already names the CTH, so the full title would repeat it on every row.
    let name = if rec.cth.is_some() && rec.sigla != MISSING {
        rec.sigla.as_str()
    } else {
        rec.title.as_str()
    };
    let title = escape_html(name);
    let lang = escape_html(&rec.lang);
    let auth = escape_html(&rec.authorship);
    let year = escape_html(&rec.year);
    let corpus = escape_html(&rec.corpus);
    let _ = writeln!(
        out,
        "        <tr>\n          <td class=\"num\">{row_n}</td>\n          <td>{title}</td>\n          <td>{lang}</td>\n          <td>{corpus}</td>\n          <td>{auth}</td>\n          <td class=\"year\">{year}</td>\n        </tr>"
    );
}

/// The line under the title: where the data came from and how much of it there is.
fn write_summary(
    out: &mut String,
    source: &str,
    generated_at: &str,
    count: usize,
    groups: usize,
) {
    let source = escape_html(source);
    let generated = escape_html(generated_at);
    out.push_str("    <p class=\"meta\">\n");
    let _ = writeln!(out, "      <span>Source: {source}</span>");
    let _ = writeln!(out, "      <span>Generated: {generated}</span>");
    let _ = writeln!(out, "      <span>Manuscripts: {count}</span>");
    let _ = writeln!(out, "      <span>Groups (CTH): {groups}</span>");
    out.push_str("    </p>\n");
}

/// What each column holds, spelled out above the table.
fn write_legend(out: &mut String) {
    out.push_str("    <section class=\"legend\" aria-label=\"Column legend\">\n");
    out.push_str("      <p class=\"legend-title\">Columns</p>\n");
    out.push_str("      <ul class=\"legend-list\">\n");
    for column in &COLUMNS {
        let _ = writeln!(
            out,
            "        <li><span class=\"k\">{}</span><span class=\"d\">{}</span></li>",
            column.head, column.legend
        );
    }
    out.push_str("      </ul>\n");
    out.push_str("    </section>\n");
}

/// The table around the rows: column widths, headings, body.
fn write_table(out: &mut String, body_rows: &str) {
    out.push_str("    <table id=\"inv\">\n");
    out.push_str("      <colgroup>\n");
    // Three to a line, as they would be written by hand.
    for line in COLUMNS.chunks(3) {
        out.push_str("        ");
        for column in line {
            let _ = write!(out, "<col class=\"{}\" />", column.class);
        }
        out.push('\n');
    }
    out.push_str("      </colgroup>\n");
    out.push_str("      <thead>\n");
    out.push_str("        <tr>\n");
    for column in &COLUMNS {
        let _ = writeln!(out, "          <th scope=\"col\">{}</th>", column.head);
    }
    out.push_str("        </tr>\n");
    out.push_str("      </thead>\n");
    out.push_str("      <tbody>\n");
    out.push_str(body_rows);
    out.push_str("      </tbody>\n");
    out.push_str("    </table>\n");
}

/// Everything before the stylesheet.
const DOCUMENT_HEAD: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Thesaurus Linguarum Hethaeorum Digitalis</title>
  <style>
"#;

/// From the end of the stylesheet to the page title.
const HEAD_TO_BODY: &str = r#"  </style>
</head>
<body>
  <main>
    <h1>Thesaurus Linguarum Hethaeorum Digitalis</h1>
"#;

/// The search box. Filtering itself is `html_filter.js`.
const TOOLBAR: &str = r#"    <div class="toolbar">
      <input type="search" id="q" placeholder="Search CTH, siglum, lang, corpus, editor, year…" autocomplete="off" spellcheck="false" />
      <span class="hint" id="hint"></span>
    </div>
"#;

/// From the end of the table to the start of the script.
const BODY_TO_SCRIPT: &str = r#"  </main>
  <script>
"#;

/// Everything after the script.
const DOCUMENT_TAIL: &str = r#"  </script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::MISSING;

    fn rec(
        sigla: &str,
        cth: Option<&str>,
        cth_num: u32,
        auth: &str,
        year: &str,
    ) -> ManuscriptRecord {
        let title = match cth {
            Some(c) => format!("{sigla} · {c}"),
            None => sigla.to_string(),
        };
        ManuscriptRecord {
            title,
            sigla: sigla.into(),
            cth: cth.map(|s| s.to_string()),
            cth_num,
            authorship: auth.into(),
            year: year.into(),
            lang: "Hit".into(),
            inv: "—".into(),
            corpus: "HFR".into(),
        }
    }

    #[test]
    fn escapes_dangerous_characters() {
        let s = escape_html("a<b>&\"c'");
        assert!(s.contains("&lt;"));
        assert!(s.contains("&gt;"));
        assert!(s.contains("&amp;"));
        assert!(s.contains("&quot;"));
        assert!(s.contains("&#39;"));
    }

    #[test]
    fn groups_same_cth_together() {
        let records = vec![
            rec("KBo 2", Some("CTH 547"), 547, "A", "2020"),
            rec("KBo 1", Some("CTH 547"), 547, "B", "2019"),
            rec("KUB 1", Some("CTH 100"), 100, "C", "2018"),
        ];
        // Pretend already sorted by cth_num then sigla
        let mut records = records;
        records.sort_by(|a, b| {
            a.cth_num
                .cmp(&b.cth_num)
                .then_with(|| a.sigla.cmp(&b.sigla))
        });
        let html = render_html(&records, "src", "now");
        assert!(html.contains("class=\"group\""));
        assert!(html.contains("CTH 100"));
        assert!(html.contains("CTH 547"));
        assert!(html.contains("Groups (CTH): 2"));
        // CTH 100 section before 547
        let p100 = html.find("CTH 100").unwrap();
        let p547 = html.find("CTH 547").unwrap();
        assert!(p100 < p547);
        // Within 547, KBo 1 before KBo 2 (sigla)
        let k1 = html.find("KBo 1").unwrap();
        let k2 = html.find("KBo 2").unwrap();
        assert!(k1 < k2);
        // Group rows use siglum only, not full "· CTH"
        assert!(!html.contains("KBo 1 · CTH"));
    }

    #[test]
    fn html_contains_structure_and_rows() {
        let records = vec![
            rec("KBo 1", Some("CTH 1"), 1, "SG", "2020"),
            ManuscriptRecord {
                title: "X <script>alert(1)</script>".into(),
                sigla: "X <script>alert(1)</script>".into(),
                cth: None,
                cth_num: u32::MAX,
                authorship: MISSING.into(),
                year: "2019".into(),
                lang: "Hit".into(),
                inv: "—".into(),
                corpus: "HFR".into(),
            },
        ];
        let html = render_html(&records, "Zenodo 20328284", "2026-08-10 12:00:00");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Thesaurus Linguarum Hethaeorum Digitalis"));
        assert!(html.contains("#fafafa"));
        assert!(html.contains("system-ui"));
        assert!(html.contains("KBo 1"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("Manuscripts: 2"));
        assert!(html.contains("Zenodo 20328284"));
        assert!(html.contains("class=\"num\">1</td>"));
    }

    #[test]
    fn empty_table_still_valid() {
        let html = render_html(&[], "src", "now");
        assert!(html.contains("Manuscripts: 0"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("Groups (CTH): 0"));
    }

    /// The legend, the column widths and the headings are generated from one
    /// list, so a column cannot appear in two of the three and be missing from
    /// the last — which is what writing them out separately allowed.
    #[test]
    fn every_column_appears_in_all_three_places() {
        let html = render_html(&[], "src", "now");
        for column in &COLUMNS {
            assert!(
                html.contains(&format!("<th scope=\"col\">{}</th>", column.head)),
                "no heading for {}",
                column.head
            );
            assert!(
                html.contains(&format!("<col class=\"{}\" />", column.class)),
                "no colgroup entry for {}",
                column.class
            );
            assert!(
                html.contains(&format!("<span class=\"d\">{}</span>", column.legend)),
                "no legend entry for {}",
                column.head
            );
        }
    }
}
