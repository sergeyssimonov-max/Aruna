//! The inventory the package opens with.
//!
//! Not a second renderer: the package gets the same document the CLI writes —
//! the same table, the same search, the same folding, the same attribution —
//! with every group and every manuscript turned into a link. The rendering
//! itself belongs to [`crate::html`], and this module only says where the links
//! point.
//!
//! Pure: records and their placements in, one HTML document out. It reads
//! nothing and writes nothing, so what it produces can be checked against a
//! string rather than against a folder on disk.

use super::naming::{dir_component, href};
use super::Placed;
use crate::html::{escape_html, render_linked_html, Links};
use crate::parse::{group_label, group_runs, ManuscriptRecord};
use std::fmt::Write as _;
use std::path::PathBuf;

/// The package's inventory: the CLI's own, with the links a folder makes possible.
///
/// No timestamp is passed. A package is rebuilt from the same archive and has
/// to come out identical every time, and a clock reading is the one thing that
/// would not.
pub fn render_inventory(records: &[ManuscriptRecord], placed: &[Placed], source: &str) -> String {
    // One href per group, in the order the groups appear, and one per record,
    // in record order — which is the order `placed` is already in.
    // At the group's page, not at the folder: Safari renders nothing for a
    // `file://` directory, so a link to a bare folder is a blank page for
    // anyone who opens the package in it.
    let groups: Vec<String> = group_runs(records)
        .map(|run| {
            href(
                &PathBuf::from(dir_component(group_label(&run[0])))
                    .join(crate::export::GROUP_INDEX),
            )
        })
        .collect();
    let fragments: Vec<String> = placed.iter().map(|p| href(&p.relative)).collect();

    render_linked_html(
        records,
        source,
        "",
        &Links {
            groups: &groups,
            fragments: &fragments,
        },
    )
}

/// The `href="…"` values of an inventory, in the order it lists them.
///
/// Kept beside the writer rather than in the validator: the two are the same
/// question asked in opposite directions, and a change to how a link is written
/// has to be answered here in the same commit.
pub fn hrefs(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("href=\"") {
        rest = &rest[at + 6..];
        let Some(end) = rest.find('"') else { break };
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    out
}

/// The listing a CTH folder opens with.
///
/// Safari does not render `file://` directory listings — observed, not assumed:
/// opening a folder URL yields a document with no title, no URL and no source,
/// which is a blank page for the reader. Chrome does render one. A package that
/// is meant to be opened by double-clicking cannot depend on which browser that
/// is, so every group gets a page of its own and the group link points at it.
///
/// Written from the same model as the inventory, with the same link rules, so
/// the two cannot disagree about where a document is.
pub fn render_group_index(group: &str, run: &[ManuscriptRecord], placed: &[Placed]) -> String {
    let mut html = String::with_capacity(1024 + run.len() * 160);
    html.push_str(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
    );
    let _ = writeln!(
        html,
        "<title>{} — TLHdig Beta 0.3</title>",
        escape_html(group)
    );
    html.push_str(
        "<style>\n:root{color-scheme:light dark}\n\
         body{font:15px/1.6 system-ui,-apple-system,sans-serif;margin:0;padding:2rem;\
         background:#fafafa;color:#1a1a1a}\n\
         @media (prefers-color-scheme:dark){body{background:#141414;color:#e8e8e8}}\n\
         h1{font-size:1.15rem;margin:0 0 .2rem}\n\
         p.meta{color:#888;font-size:.82rem;margin:0 0 1.4rem}\n\
         ul{list-style:none;margin:0;padding:0}\nli{padding:.15rem 0}\n\
         a{color:inherit}\n.dim{color:#888;font-size:.85em;margin-left:.5rem}\n\
         </style>\n</head>\n<body>\n",
    );
    let _ = writeln!(html, "<h1>{}</h1>", escape_html(group));
    let _ = writeln!(
        html,
        "<p class=\"meta\">{} manuscript{} · <a href=\"../{}.html\">back to the inventory</a></p>\n<ul>",
        run.len(),
        if run.len() == 1 { "" } else { "s" },
        crate::export::PACKAGE
    );

    for (record, place) in run.iter().zip(placed) {
        // Relative to this folder, so the page works wherever the package is
        // moved and whatever it is opened from.
        let file = place
            .relative
            .file_name()
            .map(|name| PathBuf::from(name.to_os_string()))
            .unwrap_or_else(|| place.relative.clone());
        let _ = writeln!(
            html,
            "  <li><a href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a>\
             <span class=\"dim\">{} · {} · {}</span></li>",
            href(&file),
            escape_html(&place.label),
            escape_html(&record.lang),
            escape_html(&record.authorship),
            escape_html(&record.year)
        );
    }
    html.push_str("</ul>\n</body>\n</html>\n");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::place;
    use crate::export::tests_support::fragment;

    fn built(fragments: &[crate::export::Fragment]) -> (String, Vec<Placed>) {
        let placed = place(fragments).expect("placed");
        let records: Vec<ManuscriptRecord> = fragments.iter().map(|f| f.record.clone()).collect();
        (render_inventory(&records, &placed, "test"), placed)
    }

    #[test]
    fn every_link_in_the_inventory_opens_in_a_new_context() {
        let (html, _) = built(&[fragment(
            "KBo 17.86+",
            "CTH 786",
            "root/CTH 786_XML_HFR/x.xml",
        )]);

        assert_eq!(
            html.matches("target=\"_blank\"").count(),
            2,
            "group and fragment"
        );
        assert_eq!(html.matches("rel=\"noopener\"").count(), 2);
        assert!(
            html.contains("href=\"./CTH%20786/index.html\""),
            "the group link points at its page, not the bare folder"
        );
        assert!(html.contains("href=\"./CTH%20786/KBo%2017.86%2B.xml\""));
        // No absolute path may reach the document.
        assert!(!html.contains("file://"));
        assert!(!html.contains("/Users/"));
    }

    /// What the package's inventory is for: everything the CLI's inventory has,
    /// not a stripped-down list beside it.
    #[test]
    fn the_package_gets_the_whole_inventory_and_not_a_summary_of_one() {
        let (html, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);

        assert!(html.contains("type=\"search\" id=\"q\""), "search field");
        assert!(html.contains("group-toggle"), "the fold control");
        assert!(html.contains("id=\"fold-all\""), "fold everything");
        assert!(html.contains("Corpus authors:"), "attribution");
        assert!(html.contains("Manuscripts:"), "the counts");
        assert!(html.contains("legend"), "the column legend");
        assert!(
            html.contains("EDITOR_ALIASES"),
            "the search knows the editors"
        );
    }

    /// The label is a link and the chevron is a button, and they are siblings:
    /// an anchor inside a button is invalid, and the script folds on the button
    /// alone, so a click on the label opens the folder instead of folding.
    #[test]
    fn folding_and_following_a_link_are_separate_controls() {
        let (html, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);

        assert!(html.contains("<span class=\"group-head\">"));
        assert!(
            html.contains("</button><a class=\"group-label\""),
            "the label sits after the button, not inside it"
        );
        assert!(
            !html.contains("<a class=\"group-label\" href=\"./CTH%205\" target=\"_blank\" rel=\"noopener\">CTH 5</a></button>"),
            "the anchor must not be inside the button"
        );
    }

    /// A package has to be reproducible, so its inventory carries no clock.
    #[test]
    fn the_package_inventory_has_no_timestamp_in_it() {
        let (first, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);
        let (second, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);
        assert!(!first.contains("Generated:"), "a clock reading got in");
        assert_eq!(first, second);
    }

    /// The reader of an inventory and its writer have to agree about what a
    /// link is, so the extraction is checked against a document this module
    /// wrote rather than against a hand-made string.
    #[test]
    fn the_links_read_back_are_the_links_written() {
        let (html, placed) = built(&[
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("Bo 2023/23", "CTH 5", "root/CTH 5_XML_HFR/b.xml"),
        ]);

        let found = hrefs(&html);
        assert_eq!(found.len(), 3, "one group and two fragments: {found:?}");
        for place in &placed {
            let want = href(&place.relative);
            assert!(found.contains(&want.as_str()), "{want} was not linked");
        }
    }
}
