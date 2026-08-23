//! The inventory the package opens with.
//!
//! Not a second renderer: the package gets the same document the CLI writes —
//! the same table, the same search, the same folding, the same attribution —
//! with every manuscript turned into a link to its own XML file. The rendering
//! itself belongs to [`crate::html`], and this module only says where the links
//! point.
//!
//! Until 2026-08-23 it also wrote a page for every CTH folder and linked the
//! group headings at those. That was given up: a CTH label is a way of reading
//! the table, not a document, and a reader who wants a manuscript wants the
//! manuscript. Nothing here writes a `index.html` any more, and
//! `package_pages.rs` fails if one appears.
//!
//! Pure: records and their placements in, one HTML document out. It reads
//! nothing and writes nothing, so what it produces can be checked against a
//! string rather than against a folder on disk.

use super::Placed;
use crate::html::render_linked_html;
use crate::parse::ManuscriptRecord;
use crate::presentation::CorpusPresentation;

/// The package's inventory: the CLI's own, with the links a folder makes possible.
///
/// No timestamp is passed. A package is rebuilt from the same archive and has
/// to come out identical every time, and a clock reading is the one thing that
/// would not.
pub fn render_inventory(records: &[ManuscriptRecord], placed: &[Placed], source: &str) -> String {
    // Where every link points is [`crate::presentation`]'s decision, made once
    // for the package rather than once per document. At the XML file itself —
    // never at a folder, which Safari renders as a blank page for a `file://`
    // URL, and never at a page about the folder, which is the thing this
    // stopped producing.
    render_linked_html(&CorpusPresentation::linked(records, placed, source), "")
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
            1,
            "the fragment, and nothing else"
        );
        assert_eq!(html.matches("rel=\"noopener\"").count(), 1);
        assert!(html.contains("href=\"./CTH%20786/KBo%2017.86%2B.xml\""));
        assert!(
            !html.contains("index.html"),
            "a CTH folder has no page, and nothing may link to one"
        );
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

    /// A CTH heading is text inside its fold button, and carries no link.
    ///
    /// It was an anchor beside the button while the folders had pages. Now that
    /// they do not, the whole heading folds again — which is what the client
    /// script was written against, and what it still does now that
    /// `frontend/` builds it.
    #[test]
    fn a_group_heading_folds_and_does_not_link() {
        let (html, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);

        assert!(
            html.contains("<span class=\"group-label\">CTH 5</span>"),
            "the label is text"
        );
        assert!(
            !html.contains("<a class=\"group-label\""),
            "a CTH label must never be a link"
        );
        assert!(
            !html.contains("group-head"),
            "the wrapper existed only to sit an anchor beside the button"
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
        assert_eq!(found.len(), 2, "two fragments and no group: {found:?}");
        for place in &placed {
            let want = super::super::naming::href(&place.relative);
            assert!(found.contains(&want.as_str()), "{want} was not linked");
        }
    }
}
