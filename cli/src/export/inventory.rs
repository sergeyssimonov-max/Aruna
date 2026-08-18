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
use crate::html::{render_linked_html, Links};
use crate::parse::{group_label, group_runs, ManuscriptRecord};
use std::path::PathBuf;

/// The package's inventory: the CLI's own, with the links a folder makes possible.
///
/// No timestamp is passed. A package is rebuilt from the same archive and has
/// to come out identical every time, and a clock reading is the one thing that
/// would not.
pub fn render_inventory(records: &[ManuscriptRecord], placed: &[Placed], source: &str) -> String {
    // One href per group, in the order the groups appear, and one per record,
    // in record order — which is the order `placed` is already in.
    let groups: Vec<String> = group_runs(records)
        .map(|run| href(&PathBuf::from(dir_component(group_label(&run[0])))))
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
        assert!(html.contains("href=\"./CTH%20786\""), "group link");
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
