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
        legend: "languages, most-used first (Hit, Hur…)",
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
/// Where an inventory's groups and manuscripts point, when it is part of a
/// package that has them on disk beside it.
///
/// Two slices rather than a lookup: the rows are written in one pass over
/// records that are already in display order, so the href for row *n* is the
/// *n*-th entry and nothing has to be searched for. Both are indexed by
/// position, which is why they are built from the same ordering the rows are.
pub struct Links<'a> {
    /// One href per group, in the order the groups appear.
    pub groups: &'a [String],
    /// One href per record, in record order.
    pub fragments: &'a [String],
}

/// The inventory as the CLI writes it: a document that stands on its own.
pub fn render_html(records: &[ManuscriptRecord], source: &str, generated_at: &str) -> String {
    render(records, source, generated_at, None)
}

/// The same inventory, with every group and every manuscript a link.
///
/// The same function on purpose. A package's inventory needs the search, the
/// folding and the attribution this one already has, and a second renderer
/// beside it would be a second description of the same table — which this
/// project has paid for before, in two halves that quietly stopped agreeing.
pub fn render_linked_html(
    records: &[ManuscriptRecord],
    source: &str,
    generated_at: &str,
    links: &Links<'_>,
) -> String {
    render(records, source, generated_at, Some(links))
}

fn render(
    records: &[ManuscriptRecord],
    source: &str,
    generated_at: &str,
    links: Option<&Links<'_>>,
) -> String {
    let (body_rows, groups) = render_rows(records, links);

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
/// The records arrive sorted (`order::sort_records`), so a group is a run of
/// equal labels rather than something to collect into a map.
fn render_rows(records: &[ManuscriptRecord], links: Option<&Links<'_>>) -> (String, usize) {
    let mut rows = String::new();
    let mut row_n = 0usize;
    let mut groups = 0usize;

    for run in group_runs(records) {
        let group_href = links.and_then(|l| l.groups.get(groups)).map(String::as_str);
        groups += 1;
        write_group_row(&mut rows, group_label(&run[0]), run.len(), group_href);

        for rec in run {
            let href = links
                .and_then(|l| l.fragments.get(row_n))
                .map(String::as_str);
            row_n += 1;
            write_item_row(&mut rows, row_n, rec, href);
        }
    }

    (rows, groups)
}

/// A section heading, and the control that folds the manuscripts under it.
///
/// The whole heading is a `<button>` rather than a row with a click handler, so
/// the group can be folded from the keyboard and a screen reader is told what
/// the control does and what state it is in. `aria-expanded` starts `true`
/// because a document opened without JavaScript shows everything.
fn write_group_row(out: &mut String, label: &str, count: usize, href: Option<&str>) {
    let label = escape_html(label);
    let Some(href) = href else {
        let _ = writeln!(
            out,
            "        <tr class=\"group\">\n          <td colspan=\"6\"><button type=\"button\" class=\"group-toggle\" aria-expanded=\"true\"><span class=\"chevron\" aria-hidden=\"true\"></span><span class=\"group-label\">{label}</span><span class=\"group-count\">{count}</span></button></td>\n        </tr>"
        );
        return;
    };

    // The label leaves the button when it becomes a link: an anchor inside a
    // button is invalid, and browsers disagree about which of the two a click
    // belongs to. Apart, each does one thing — the chevron folds the group, the
    // label opens its folder — and `.group-head` restores the spacing the
    // button's own flex layout was providing.
    let href = escape_html(href);
    let _ = writeln!(
        out,
        "        <tr class=\"group\">\n          <td colspan=\"6\"><span class=\"group-head\"><button type=\"button\" class=\"group-toggle\" aria-expanded=\"true\"><span class=\"chevron\" aria-hidden=\"true\"></span></button><a class=\"group-label\" href=\"{href}\" target=\"_blank\" rel=\"noopener\">{label}</a><span class=\"group-count\">{count}</span></span></td>\n        </tr>"
    );
}

fn write_item_row(out: &mut String, row_n: usize, rec: &ManuscriptRecord, href: Option<&str>) {
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
    // A link where there is one, the bare name where there is not. The cell's
    // text is the same either way, which is what the search reads.
    let title = match href {
        Some(href) => format!(
            "<a href=\"{}\" target=\"_blank\" rel=\"noopener\">{title}</a>",
            escape_html(href)
        ),
        None => title,
    };
    let _ = writeln!(
        out,
        "        <tr>\n          <td class=\"num\">{row_n}</td>\n          <td>{title}</td>\n          <td>{lang}</td>\n          <td>{corpus}</td>\n          <td>{auth}</td>\n          <td class=\"year\">{year}</td>\n        </tr>"
    );
}

/// The line under the title: where the data came from, who made it, and how
/// much of it there is.
///
/// The two counts share a line — they are one fact about the corpus, and each
/// on its own row made a four-line block out of a header.
fn write_summary(out: &mut String, source: &str, generated_at: &str, count: usize, groups: usize) {
    let source = escape_html(source);
    let generated = escape_html(generated_at);
    // Not the `Editor` column, which says who edited one manuscript. These four
    // are credited with the corpus itself, and the label has to keep the two
    // apart on a page that shows both.
    let authors = escape_html(&crate::corpus_authors_line());
    out.push_str("    <p class=\"meta\">\n");
    let _ = writeln!(out, "      <span>Source: {source}</span>");
    let _ = writeln!(out, "      <span>Corpus authors: {authors}</span>");
    // Omitted when the caller has no timestamp to give. A package is rebuilt
    // from the same archive and has to come out the same every time, and a
    // clock reading in the document would be the one thing that never did.
    if !generated.is_empty() {
        let _ = writeln!(out, "      <span>Generated: {generated}</span>");
    }
    let _ = writeln!(
        out,
        "      <span class=\"counts\"><span class=\"count\">Manuscripts: {count}</span><span class=\"count\">Groups (CTH): {groups}</span></span>"
    );
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

/// The search box and the fold-everything control. Both act through
/// `html_filter.js`; without it the document is a plain, fully expanded table.
const TOOLBAR: &str = r#"    <div class="toolbar">
      <input type="search" id="q" placeholder="Search CTH, siglum, lang, corpus, editor, year…" autocomplete="off" spellcheck="false" />
      <button type="button" id="fold-all" class="fold-all" aria-expanded="true">Collapse fragments</button>
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

    /// The credit belongs to the corpus, not to the rows, so it is there for an
    /// inventory of nothing just as much as for a full one.
    #[test]
    fn the_corpus_authors_are_credited_under_the_title() {
        let html = render_html(&[], "src", "now");
        assert!(
            html.contains(&format!(
                "<span>Corpus authors: {}</span>",
                crate::corpus_authors_line()
            )),
            "the summary block does not credit the corpus authors"
        );
    }

    #[test]
    fn empty_table_still_valid() {
        let html = render_html(&[], "src", "now");
        assert!(html.contains("Manuscripts: 0"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("Groups (CTH): 0"));
    }

    /// Folding is what the group headings are for, so every heading has to be
    /// a control — not a row that happens to have a click handler attached to
    /// it somewhere in the script.
    #[test]
    fn every_group_heading_is_a_button_the_keyboard_can_reach() {
        let records = vec![
            rec("KBo 1", Some("CTH 1"), 1, "A", "2020"),
            rec("KUB 9", Some("CTH 547"), 547, "B", "2021"),
        ];
        let html = render_html(&records, "src", "now");

        assert_eq!(
            html.matches("<button type=\"button\" class=\"group-toggle\" aria-expanded=\"true\">")
                .count(),
            2,
            "one control per CTH group, open by default"
        );
        assert_eq!(
            html.matches("class=\"chevron\"").count(),
            2,
            "each heading shows which way it is folded"
        );
        assert!(
            html.contains("id=\"fold-all\""),
            "the toolbar carries the fold-everything control"
        );
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
