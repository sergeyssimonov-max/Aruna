//! Scandinavian-style HTML inventory generation.
//!
//! Rows are grouped by CTH catalogue number (tablet family); fragments of the
//! same text (e.g. all CTH 547) render under one section heading.

use crate::parse::{group_label, ManuscriptRecord, MISSING};
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

/// Build the full HTML document.
///
/// `source` — human-readable source line (Zenodo record).
/// `generated_at` — already-formatted local date/time string.
pub fn render_html(
    records: &[ManuscriptRecord],
    source: &str,
    generated_at: &str,
) -> String {
    let mut body_rows = String::new();
    let mut row_n = 0usize;
    let mut groups = 0usize;
    let mut i = 0usize;

    while i < records.len() {
        let label = group_label(&records[i]).to_string();
        let mut j = i + 1;
        while j < records.len() && group_label(&records[j]) == label {
            j += 1;
        }
        let group_count = j - i;
        groups += 1;

        let label_esc = escape_html(&label);
        let _ = writeln!(
            body_rows,
            "        <tr class=\"group\">\n          <td colspan=\"6\"><span class=\"group-label\">{label_esc}</span><span class=\"group-count\">{group_count}</span></td>\n        </tr>"
        );

        for rec in &records[i..j] {
            row_n += 1;
            // Within a CTH group show the siglum as the primary name.
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
                body_rows,
                "        <tr>\n          <td class=\"num\">{row_n}</td>\n          <td>{title}</td>\n          <td>{lang}</td>\n          <td>{corpus}</td>\n          <td>{auth}</td>\n          <td class=\"year\">{year}</td>\n        </tr>"
            );
        }
        i = j;
    }

    let count = records.len();
    let source = escape_html(source);
    let generated = escape_html(generated_at);

    let mut html = String::with_capacity(4096 + body_rows.len());
    html.push_str("<!DOCTYPE html>\n");
    html.push_str("<html lang=\"en\">\n");
    html.push_str("<head>\n");
    html.push_str("  <meta charset=\"utf-8\" />\n");
    html.push_str(
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n",
    );
    html.push_str("  <title>Thesaurus Linguarum Hethaeorum Digitalis</title>\n");
    html.push_str("  <style>\n");
    html.push_str(include_str!("html_style.css"));
    html.push_str("  </style>\n");
    html.push_str("</head>\n");
    html.push_str("<body>\n");
    html.push_str("  <main>\n");
    html.push_str("    <h1>Thesaurus Linguarum Hethaeorum Digitalis</h1>\n");
    html.push_str("    <p class=\"meta\">\n");
    let _ = writeln!(html, "      <span>Source: {source}</span>");
    let _ = writeln!(html, "      <span>Generated: {generated}</span>");
    let _ = writeln!(html, "      <span>Manuscripts: {count}</span>");
    let _ = writeln!(html, "      <span>Groups (CTH): {groups}</span>");
    html.push_str("    </p>\n");

    // Legend
    html.push_str("    <section class=\"legend\" aria-label=\"Column legend\">\n");
    html.push_str("      <p class=\"legend-title\">Columns</p>\n");
    html.push_str("      <ul class=\"legend-list\">\n");
    html.push_str("        <li><span class=\"k\">№</span><span class=\"d\">row number</span></li>\n");
    html.push_str("        <li><span class=\"k\">Siglum</span><span class=\"d\">publication id (e.g. KBo 3.22)</span></li>\n");
    html.push_str("        <li><span class=\"k\">Lang</span><span class=\"d\">dominant language (Hit, Hur, Akk…)</span></li>\n");
    html.push_str("        <li><span class=\"k\">Corpus</span><span class=\"d\">edition series (HFR, TLH, HAnn…)</span></li>\n");
    html.push_str("        <li><span class=\"k\">Editor</span><span class=\"d\">transliteration / edition author</span></li>\n");
    html.push_str("        <li><span class=\"k\">Year</span><span class=\"d\">edition year</span></li>\n");
    html.push_str("      </ul>\n");
    html.push_str("    </section>\n");

    // Search filter
    html.push_str("    <div class=\"toolbar\">\n");
    html.push_str("      <input type=\"search\" id=\"q\" placeholder=\"Search CTH, siglum, lang, corpus, editor, year…\" autocomplete=\"off\" spellcheck=\"false\" />\n");
    html.push_str("      <span class=\"hint\" id=\"hint\"></span>\n");
    html.push_str("    </div>\n");

    html.push_str("    <table id=\"inv\">\n");
    html.push_str("      <colgroup>\n");
    html.push_str("        <col class=\"c-num\" /><col class=\"c-sig\" /><col class=\"c-lang\" />\n");
    html.push_str("        <col class=\"c-corp\" /><col class=\"c-ed\" /><col class=\"c-year\" />\n");
    html.push_str("      </colgroup>\n");
    html.push_str("      <thead>\n");
    html.push_str("        <tr>\n");
    html.push_str("          <th scope=\"col\">№</th>\n");
    html.push_str("          <th scope=\"col\">Siglum</th>\n");
    html.push_str("          <th scope=\"col\">Lang</th>\n");
    html.push_str("          <th scope=\"col\">Corpus</th>\n");
    html.push_str("          <th scope=\"col\">Editor</th>\n");
    html.push_str("          <th scope=\"col\">Year</th>\n");
    html.push_str("        </tr>\n");
    html.push_str("      </thead>\n");
    html.push_str("      <tbody>\n");
    html.push_str(&body_rows);
    html.push_str("      </tbody>\n");
    html.push_str("    </table>\n");
    html.push_str("  </main>\n");
    html.push_str("  <script>\n");
    html.push_str(include_str!("html_filter.js"));
    html.push_str("  </script>\n");
    html.push_str("</body>\n");
    html.push_str("</html>\n");
    html

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::MISSING;

    fn rec(sigla: &str, cth: Option<&str>, cth_num: u32, auth: &str, year: &str) -> ManuscriptRecord {
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
}
