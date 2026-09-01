//! Scandinavian-style HTML inventory generation.
//!
//! Rows are grouped by CTH catalogue number (tablet family); fragments of the
//! same text (e.g. all CTH 547) render under one section heading.
//!
//! **The markup is not written here.** It is authored in
//! `frontend/src/inventory/` as Svelte components, rendered once at build time
//! into `generated/*.html` with a placeholder wherever data goes, and compiled
//! in below — the same arrangement the stylesheet and the client script already
//! had, and the last of the three parts of the target state
//! (`docs/FRONTEND-CONTRACT.md`, *The target state*). What is left in this file
//! is the substitution: which record fills which hole, in what order, and how
//! many of each fragment there are.
//!
//! The document is still assembled from three kinds of thing, kept apart on
//! purpose: [`DOCUMENT`], which is the page as the frontend wrote it; the
//! fragments it repeats, one per group, manuscript and column; and [`COLUMNS`],
//! the one description of the table's columns, which stays here because a
//! second list of them in TypeScript is exactly the duplication this project
//! has paid for before.

use crate::parse::ManuscriptRecord;
use crate::presentation::{CorpusPresentation, FragmentPresentation};

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

// ---------------------------------------------------------------------------
// What the frontend built
// ---------------------------------------------------------------------------

/// The page itself, from the doctype to `</html>`, with a hole in it wherever
/// a corpus goes.
///
/// A build product, like the stylesheet sections and the script beside it.
/// `frontend/build/inventory.ts` renders `Document.svelte` with every prop set
/// to its own placeholder and writes the result here; the artifact is committed
/// so that `cargo build` never needs Node, which is the premise of the `.app`
/// and the DMG, and `frontend/tests/inventory-artifact.test.ts` fails if what is
/// committed is not what the sources now produce.
const DOCUMENT: &str = include_str!("generated/document.html");

/// A CTH section heading, and the control that folds the manuscripts under it.
///
/// The whole heading is a `<button>` rather than a row with a click handler, so
/// the group can be folded from the keyboard and a screen reader is told what
/// the control does and what state it is in. `aria-expanded` starts `true`
/// because a document opened without JavaScript shows everything.
///
/// A CTH label is text, never a link. Grouping is a way of reading the table,
/// not a destination: the folders hold XML files and the rows under this
/// heading link straight at them. Group pages existed until 2026-08-23 and were
/// given up deliberately — see `docs/FRONTEND-CONTRACT.md`.
const GROUP_HEADING: &str = include_str!("generated/group_heading.html");

/// One manuscript: the six cells, in the order [`COLUMNS`] names them.
const MANUSCRIPT_ROW: &str = include_str!("generated/manuscript_row.html");

/// The title cell's contents when there is a file to link at.
const MANUSCRIPT_LINK: &str = include_str!("generated/manuscript_link.html");

/// One `<col>`, which the stylesheet gives its width.
const COLUMN_WIDTH: &str = include_str!("generated/column_width.html");

/// One `<th>`.
const COLUMN_HEADING: &str = include_str!("generated/column_heading.html");

/// One line of the legend above the table.
const LEGEND_ENTRY: &str = include_str!("generated/legend_entry.html");

/// The `Generated:` line, which a package's inventory does not carry.
const GENERATED_LINE: &str = include_str!("generated/generated_line.html");

/// The client script the inventory carries: the search box and the folding.
///
/// Vite bundles `frontend/src/inventory/main.ts` into it, in the same build and
/// on the same terms as the markup above.
const INVENTORY_SCRIPT: &str = include_str!("generated/inventory_filter.js");

// ---------------------------------------------------------------------------
// Filling the holes
// ---------------------------------------------------------------------------

/// The two characters that fence a placeholder: `@@ROWS@@`.
///
/// Chosen because nothing in HTML, in a stylesheet, in the script or in the
/// corpus's own text means anything by them, and because a placeholder that
/// survives into a document is then obvious on sight rather than invisible.
const FENCE: &str = "@@";

/// Substitute into a fragment, and append the result.
///
/// A value is written out as it stands: whatever a record contributes has been
/// through [`escape_html`] before it gets here, and it is never scanned for
/// placeholders of its own — a manuscript titled `@@ROWS@@` is a title.
///
/// A placeholder this caller has no value for is left as the frontend wrote it
/// rather than silently dropped. That cannot happen — every template's holes
/// are filled a few lines below, and a test asserts a rendered document has
/// none left — and if it ever does, the document says so instead of quietly
/// missing a line.
fn fill(out: &mut String, template: &str, values: &[(&str, &str)]) {
    let mut rest = template;
    while let Some(open) = rest.find(FENCE) {
        let after = &rest[open + FENCE.len()..];
        let Some(close) = after.find(FENCE) else {
            break;
        };
        let name = &after[..close];
        out.push_str(&rest[..open]);
        match values.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push_str(FENCE);
                out.push_str(name);
                out.push_str(FENCE);
            }
        }
        rest = &after[close + FENCE.len()..];
    }
    out.push_str(rest);
}

/// [`fill`], for a fragment that is going to be a value in its turn.
fn filled(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    fill(&mut out, template, values);
    out
}

/// A fragment without the newline the artifact file ends in.
fn trimmed(template: &str) -> &str {
    template.trim_matches('\n')
}

/// How the document wants a list of fragments joined.
///
/// Read off the document rather than decided here. A placeholder that stands on
/// a line of its own — the rows, the legend — is asking for one fragment per
/// line, indented to where it sits; a placeholder inside a line —
/// `<colgroup>@@COLGROUP@@</colgroup>` — is asking for one line, so its
/// fragments follow one another without a break. Either way the layout of the
/// exported document is the frontend's to decide, which is the point of the
/// markup living there.
fn separator(template: &str, name: &str) -> String {
    let hole = format!("{FENCE}{name}{FENCE}");
    let Some(at) = template.find(&hole) else {
        return String::new();
    };
    let line = match template[..at].rfind('\n') {
        Some(newline) => &template[newline + 1..at],
        None => &template[..at],
    };
    if line.is_empty() || line.chars().all(|c| c == ' ') {
        format!("\n{line}")
    } else {
        String::new()
    }
}

/// Build the full HTML document.
///
/// `source` — human-readable source line (Zenodo record).
/// `generated_at` — already-formatted local date/time string.
/// The inventory as the CLI writes it: a document that stands on its own.
///
/// Nothing is linked, because nothing is placed beside it: this renderer is
/// handed a presentation without hrefs. It was what the program wrote until
/// 2.3.0, and what it writes now is the package's own inventory —
/// [`render_linked_html`] — which links at the documents around it. This one
/// remains as a library capability, exercised by `tests/integration.rs`.
pub fn render_html(records: &[ManuscriptRecord], source: &str, generated_at: &str) -> String {
    render(&CorpusPresentation::plain(records, source), generated_at)
}

/// The same inventory, with every manuscript a link to its own XML file.
///
/// The same function on purpose. A package's inventory needs the search, the
/// folding and the attribution this one already has, and a second renderer
/// beside it would be a second description of the same table — which this
/// project has paid for before, in two halves that quietly stopped agreeing.
///
/// The difference between the two is entirely in the presentation it is handed:
/// one has hrefs and the other does not, and this renderer never asks why.
pub fn render_linked_html(corpus: &CorpusPresentation<'_>, generated_at: &str) -> String {
    render(corpus, generated_at)
}

fn render(corpus: &CorpusPresentation<'_>, generated_at: &str) -> String {
    let (rows, groups) = render_rows(corpus);
    // The stylesheet is assembled in [`crate::style`] rather than held here.
    // It was shared with the CTH group pages until those were given up on
    // 2026-08-23; the seam stays because the print and screen rules are still
    // built from one source.
    let css = crate::style::inventory_css();
    // Not the `Editor` column, which says who edited one manuscript. These four
    // are credited with the corpus itself, and the label has to keep the two
    // apart on a page that shows both.
    let authors = escape_html(&crate::corpus_authors_line());
    filled(
        DOCUMENT,
        &[
            ("STYLE", &css),
            ("SCRIPT", INVENTORY_SCRIPT),
            ("SOURCE", &escape_html(corpus.source)),
            ("AUTHORS", &authors),
            ("GENERATED", &generated_line(generated_at)),
            ("MANUSCRIPTS", &corpus.manuscripts().to_string()),
            ("GROUPS", &groups.to_string()),
            ("LEGEND", &legend()),
            ("COLGROUP", &colgroup()),
            ("THEAD", &thead()),
            ("ROWS", &rows),
        ],
    )
}

/// The table body: one section row per CTH group, then its manuscripts.
///
/// Returns the rows and how many groups they fell into — the count is part of
/// the summary line, and counting it here means walking the records once.
///
/// The records arrive sorted (`order::sort_records`), so a group is a run of
/// equal labels rather than something to collect into a map.
fn render_rows(corpus: &CorpusPresentation<'_>) -> (String, usize) {
    let between = separator(DOCUMENT, "ROWS");
    let mut rows = String::new();
    let mut row_n = 0usize;

    for group in &corpus.groups {
        if !rows.is_empty() {
            rows.push_str(&between);
        }
        write_group_row(&mut rows, group.label, group.fragments.len());

        for fragment in &group.fragments {
            row_n += 1;
            rows.push_str(&between);
            write_item_row(&mut rows, row_n, fragment, fragment.href.as_deref());
        }
    }

    (rows, corpus.groups.len())
}

/// A section heading, and how many manuscripts it stands for.
///
/// The cell spans the table, which is [`COLUMNS`]'s business and not the
/// template's: `colspan` is a hole like any other.
fn write_group_row(out: &mut String, label: &str, count: usize) {
    fill(
        out,
        trimmed(GROUP_HEADING),
        &[
            ("SPAN", &COLUMNS.len().to_string()),
            ("LABEL", &escape_html(label)),
            ("COUNT", &count.to_string()),
        ],
    );
}

fn write_item_row(
    out: &mut String,
    row_n: usize,
    fragment: &FragmentPresentation<'_>,
    href: Option<&str>,
) {
    // Which name a manuscript is listed under is [`crate::presentation`]'s
    // decision, made once for every document rather than here.
    let rec = fragment.record;
    let title = escape_html(fragment.display_name);
    // A link where there is one, the bare name where there is not. The cell's
    // text is the same either way, which is what the search reads.
    let title = match href {
        Some(href) => filled(
            trimmed(MANUSCRIPT_LINK),
            &[("HREF", &escape_html(href)), ("TITLE", &title)],
        ),
        None => title,
    };
    fill(
        out,
        trimmed(MANUSCRIPT_ROW),
        &[
            ("NUMBER", &row_n.to_string()),
            ("TITLE", &title),
            ("LANG", &escape_html(&rec.lang)),
            ("CORPUS", &escape_html(&rec.corpus)),
            ("EDITOR", &escape_html(&rec.authorship)),
            ("YEAR", &escape_html(&rec.year)),
        ],
    );
}

/// The line under the title that says when the document was written.
///
/// Omitted when the caller has no timestamp to give. A package is rebuilt from
/// the same archive and has to come out the same every time, and a clock
/// reading in the document would be the one thing that never did.
fn generated_line(generated_at: &str) -> String {
    if generated_at.is_empty() {
        return String::new();
    }
    filled(
        trimmed(GENERATED_LINE),
        &[("GENERATED", &escape_html(generated_at))],
    )
}

/// The fragments of one template, one per column, joined as the document asks.
fn per_column(hole: &str, entry: impl Fn(&Column) -> String) -> String {
    let between = separator(DOCUMENT, hole);
    let mut out = String::new();
    for column in &COLUMNS {
        if !out.is_empty() {
            out.push_str(&between);
        }
        out.push_str(&entry(column));
    }
    out
}

/// What each column holds, spelled out above the table.
fn legend() -> String {
    per_column("LEGEND", |column| {
        filled(
            trimmed(LEGEND_ENTRY),
            &[("HEAD", column.head), ("LEGEND", column.legend)],
        )
    })
}

/// The column widths.
fn colgroup() -> String {
    per_column("COLGROUP", |column| {
        filled(trimmed(COLUMN_WIDTH), &[("CLASSNAME", column.class)])
    })
}

/// The column headings.
fn thead() -> String {
    per_column("THEAD", |column| {
        filled(trimmed(COLUMN_HEADING), &[("HEAD", column.head)])
    })
}

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
        assert_eq!(
            html.matches(&format!("colspan=\"{}\"", COLUMNS.len()))
                .count(),
            2,
            "a heading spans the table, however many columns it has"
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
                html.contains(&format!("<col class=\"{}\"/>", column.class)),
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

    /// Every hole the frontend left is one this file fills.
    ///
    /// The templates and the substitutions are two lists of names that have to
    /// agree, kept apart by a language boundary and by a build; nothing but this
    /// notices when one of them is renamed. What it would otherwise cost is a
    /// document that is valid, opens, and has a sentence missing from it.
    #[test]
    fn no_placeholder_survives_into_a_document() {
        let records = vec![rec("KBo 1", Some("CTH 1"), 1, "A", "2020")];
        for generated in ["", "2026-08-10 12:00:00"] {
            let html = render_html(&records, "src", generated);
            assert!(
                !html.contains(FENCE),
                "an unfilled placeholder reached the document: {:?}",
                html.split(FENCE).nth(1)
            );
        }
    }

    /// The document decides how its own lists are laid out, and this reads that
    /// decision off it rather than repeating it.
    #[test]
    fn a_list_is_joined_the_way_the_document_asks() {
        assert_eq!(separator("<tbody>@@ROWS@@</tbody>", "ROWS"), "");
        assert_eq!(separator("  <p>\n    @@ROWS@@\n  </p>", "ROWS"), "\n    ");
        assert_eq!(separator("<p>@@OTHER@@</p>", "ROWS"), "");
    }

    /// A value is written out as it stands. The corpus is other people's text,
    /// and text that happens to look like a placeholder is still text.
    #[test]
    fn a_value_that_looks_like_a_placeholder_is_not_one() {
        let mut out = String::new();
        fill(&mut out, "<td>@@TITLE@@</td>", &[("TITLE", "@@ROWS@@")]);
        assert_eq!(out, "<td>@@ROWS@@</td>");
    }
}
