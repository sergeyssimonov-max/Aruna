//! The inventory the package opens with.
//!
//! Not a second renderer: the package gets the same document the CLI writes —
//! the same table, the same search, the same folding, the same attribution —
//! with every manuscript's name turned into a link to its own PDF, or to its
//! XML where no PDF was built. The rendering itself belongs to
//! [`crate::html`], and this module only says where the links point.
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

use super::{PdfState, Placed};
use crate::html::render_linked_html;
use crate::parse::ManuscriptRecord;
use crate::presentation::CorpusPresentation;

/// The package's inventory: the CLI's own, with the links a folder makes
/// possible – each document's name leads to its PDF, or to its XML when the
/// PDF was refused (owner's decision of 2026-10-19).
///
/// `pdfs` is parallel to `placed`. A refused PDF is not marked; why it was
/// refused is the manifest's to say. This is the function the build calls,
/// so that a benchmark or a fuzzer calls the same one.
///
/// No timestamp is passed. A package is rebuilt from the same archive and has
/// to come out identical every time, and a clock reading is the one thing that
/// would not.
pub fn render_package_inventory(
    records: &[ManuscriptRecord],
    placed: &[Placed],
    source: &str,
    pdfs: &[PdfState],
) -> String {
    // Where every link points is [`crate::presentation`]'s decision, made once
    // for the package rather than once per document. At the XML file itself –
    // never at a folder, which Safari renders as a blank page for a `file://`
    // URL, and never at a page about the folder, which is the thing this
    // stopped producing.
    let corpus = CorpusPresentation::linked(records, placed, source).with_pdfs(placed, pdfs);
    render_linked_html(&corpus, "")
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

/// Where a manuscript's row begins: the cell with its number, which no other
/// row of the inventory carries. `manuscript_row.html` is held to it by a test
/// below.
const ROW_START: &str = "<td class=\"num\">";

/// The `href="…"` values of an inventory, row by row: those before the first
/// manuscript's row, then each manuscript's own, in the order the inventory
/// lists them.
///
/// The reading of [`hrefs`], cut where a row begins, so that a link can be
/// held to the document whose row it sits in. Text out of the archive is
/// escaped before it is written, so neither `href="` nor a row's first cell
/// can come out of it. What follows the last row is read as part of it: a
/// link after the table is a second link of the last document.
pub(crate) fn links_by_row(html: &str) -> (Vec<&str>, Vec<Vec<&str>>) {
    let mut pieces = html.split(ROW_START);
    let before = pieces.next().map(hrefs).unwrap_or_default();
    (before, pieces.map(hrefs).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::place;
    use crate::export::tests_support::fragment;

    /// The inventory of `fragments`, every PDF refused: each row carries the
    /// link at its XML, which is what most of these tests read. [`built_with`]
    /// gives the others.
    fn built(fragments: &[crate::export::Fragment]) -> (String, Vec<Placed>) {
        let refused: Vec<usize> = (0..fragments.len()).collect();
        let (html, placed, _) = built_with(fragments, &refused);
        (html, placed)
    }

    /// The inventory of `fragments` with the PDF of those at `refused`
    /// refused and every other one built, and what became of each.
    fn built_with(
        fragments: &[crate::export::Fragment],
        refused: &[usize],
    ) -> (String, Vec<Placed>, Vec<PdfState>) {
        let placed = place(fragments).expect("placed");
        let records: Vec<ManuscriptRecord> = fragments.iter().map(|f| f.record.clone()).collect();
        let pdfs: Vec<PdfState> = placed
            .iter()
            .enumerate()
            .map(|(i, place)| {
                if refused.contains(&i) {
                    PdfState::Refused("the document model: a fixture".into())
                } else {
                    PdfState::Built(crate::export::naming::pdf_path(&place.relative))
                }
            })
            .collect();
        (
            render_package_inventory(&records, &placed, "test", &pdfs),
            placed,
            pdfs,
        )
    }

    /// The three ways a set of documents can come out: every PDF refused,
    /// every PDF built, and the two mixed.
    fn variants(n: usize) -> [Vec<usize>; 3] {
        [
            (0..n).collect(),
            Vec::new(),
            (0..n).filter(|i| i % 2 == 1).collect(),
        ]
    }

    /// The file a document's link has to name: its PDF where one was built.
    fn target(place: &Placed, state: &PdfState) -> String {
        match state.built() {
            Some(pdf) => crate::export::naming::href(pdf),
            None => crate::export::naming::href(&place.relative),
        }
    }

    /// Three documents of one group – built, refused, built.
    fn mixed() -> Vec<crate::export::Fragment> {
        vec![
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("KBo 1.2", "CTH 5", "root/CTH 5_XML_HFR/b.xml"),
            fragment("KBo 1.3", "CTH 5", "root/CTH 5_XML_HFR/c.xml"),
        ]
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
        // And on a mixed group: every link, at a PDF or at an XML, carries
        // both attributes.
        let (html, _, _) = built_with(&mixed(), &[1]);
        assert_eq!(html.matches("<a ").count(), 3, "one link per document");
        assert_eq!(html.matches("target=\"_blank\"").count(), 3);
        assert_eq!(html.matches("rel=\"noopener\"").count(), 3);
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
            html.contains("filter-on"),
            "the client script that searches"
        );
    }

    /// **A row carries the corpus's other spellings of its editor.**
    ///
    /// The search reads a row's text and nothing else, so this is the whole of
    /// what makes `schwemer` reach the seven manuscripts that say `DS`. It used
    /// to be a list inside the client script, which the crate could state
    /// nowhere and check with `html.contains("EDITOR_ALIASES")` — that the word
    /// appeared in the bundle, not that the search would find anything.
    #[test]
    fn a_row_carries_the_other_spellings_of_its_editor() {
        let mut ds = fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml");
        ds.record.authorship = "DS".into();
        let (html, _) = built(&[ds]);

        assert!(
            html.contains("<td>DS<span hidden> Daniel Schwemer</span></td>"),
            "the spelling is written into the row, out of sight"
        );

        // An editor the corpus spells one way only gets nothing added: the
        // list says which names are the same person, not that every name has a
        // second form.
        let (plain, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);
        assert!(plain.contains("<td>AA</td>"), "no spelling, no span");
        assert!(!plain.contains("<span hidden>"));
    }

    /// **A hostile siglum reaches neither the link's text nor its address.**
    ///
    /// The package's inventory is the one document that writes an attribute out
    /// of corpus text, and an attribute is where escaping is easiest to get
    /// wrong: a quotation mark that survived would end `href="` early and leave
    /// whatever followed sitting among the tag's own attributes.
    ///
    /// Two things stand between the archive and that, and the test asks both.
    /// The address is built by `naming::href`, which percent-encodes everything
    /// outside the unreserved set, so a quotation mark cannot be in it at all;
    /// the text is escaped like any other cell.
    #[test]
    fn a_siglum_carrying_markup_reaches_neither_the_link_text_nor_its_address() {
        let mut hostile = fragment("KBo \"1<b>", "CTH 5", "root/CTH 5_XML_HFR/a.xml");
        hostile.record.title = "KBo \"1<b> · CTH 5".into();
        // At its XML and at its PDF: the link is the same attribute either way.
        for refused in variants(1) {
            let (html, _, _) = built_with(std::slice::from_ref(&hostile), &refused);

            let addresses = hrefs(&html);
            assert_eq!(addresses.len(), 1, "one manuscript, one link");
            for forbidden in ['"', '<', '>', '\''] {
                assert!(
                    !addresses[0].contains(forbidden),
                    "{forbidden} survived into the address: {}",
                    addresses[0]
                );
            }

            assert!(
                html.contains(">KBo &quot;1&lt;b&gt;</a>"),
                "the link's text is the escaped siglum and nothing else"
            );
            assert!(
                !html.contains("<b>"),
                "markup out of the archive reached the document"
            );
        }
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

        // On a mixed group the reader gives back both kinds, in the order the
        // writer wrote them, and row by row.
        let (html, placed, pdfs) = built_with(&mixed(), &[1]);
        let wanted: Vec<String> = placed
            .iter()
            .zip(&pdfs)
            .map(|(p, s)| target(p, s))
            .collect();
        assert_eq!(hrefs(&html), wanted);
        assert!(wanted[0].ends_with(".pdf") && wanted[1].ends_with(".xml"));
        let (outside, rows) = links_by_row(&html);
        assert!(outside.is_empty(), "{outside:?}");
        assert_eq!(
            rows,
            wanted.iter().map(|h| vec![h.as_str()]).collect::<Vec<_>>()
        );
    }

    /// **A document with a PDF: one link in its cell, at the PDF, on its
    /// name** (owner's decision of 2026-10-19).
    #[test]
    fn a_document_with_a_pdf_links_its_name_at_the_pdf_and_nothing_else() {
        let (html, _, _) = built_with(
            &[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")],
            &[],
        );
        let row = &html[html.find(ROW_START).expect("a row")..];
        let cell = &row[..row.find("</tr>").expect("closed")];
        assert_eq!(cell.matches("<a ").count(), 1, "{cell}");
        assert!(
            cell.contains(
                "<a href=\"./CTH%205/KBo%201.1.pdf\" target=\"_blank\" rel=\"noopener\">KBo 1.1</a>"
            ),
            "{cell}"
        );
        assert!(!html.contains("KBo%201.1.xml"), "the XML is linked as well");
    }

    /// **A document whose PDF was refused: one link, at its XML.** Not
    /// marked: why is the manifest's to say.
    #[test]
    fn a_document_with_a_refused_pdf_links_its_name_at_the_xml() {
        let (html, _) = built(&[fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml")]);
        let row = &html[html.find(ROW_START).expect("a row")..];
        let cell = &row[..row.find("</tr>").expect("closed")];
        assert_eq!(cell.matches("<a ").count(), 1, "{cell}");
        assert!(
            cell.contains(
                "<a href=\"./CTH%205/KBo%201.1.xml\" target=\"_blank\" rel=\"noopener\">KBo 1.1</a>"
            ),
            "{cell}"
        );
        assert!(!html.contains(".pdf"), "a PDF that was refused is named");
    }

    /// **Built, refused, built in one group**: the rows keep their order, and
    /// each name leads where its own PDF decided.
    #[test]
    fn a_mixed_group_links_each_document_by_its_own_pdf() {
        let (html, _, _) = built_with(&mixed(), &[1]);
        let names: Vec<usize> = ["KBo 1.1", "KBo 1.2", "KBo 1.3"]
            .iter()
            .map(|n| html.find(&format!(">{n}</a>")).expect("a linked name"))
            .collect();
        assert!(names.windows(2).all(|w| w[0] < w[1]), "{names:?}");
        let (_, rows) = links_by_row(&html);
        assert_eq!(
            rows,
            [
                ["./CTH%205/KBo%201.1.pdf"],
                ["./CTH%205/KBo%201.2.xml"],
                ["./CTH%205/KBo%201.3.pdf"],
            ]
        );
    }

    /// **No link is named `PDF`**, in any of the three ways a set can come
    /// out: the second link after the name was given up on 2026-10-19.
    #[test]
    fn no_link_in_the_inventory_is_named_pdf() {
        for refused in variants(3) {
            let (html, _, _) = built_with(&mixed(), &refused);
            assert!(!html.contains(">PDF</a>"), "{refused:?}");
            assert_eq!(html.matches("<a ").count(), 3, "{refused:?}");
        }
    }

    /// **One siglum, both ways, escaped byte for byte.** A space, a
    /// separator (escaped in the file name as `%2F`, and that `%` again in the
    /// link), `&`, `#`, `?` and a sign above the Basic Multilingual Plane –
    /// each encoded once, at the PDF and at the XML alike.
    #[test]
    fn a_link_is_escaped_once_at_the_pdf_and_at_the_xml() {
        let siglum = fragment(
            "A b/c&d #e ?f \u{12000}",
            "CTH 5",
            "root/CTH 5_XML_HFR/a.xml",
        );
        let base = "./CTH%205/A%20b%252Fc%26d%20%23e%20%3Ff%20%F0%92%80%80";
        for (refused, extension) in [(vec![], "pdf"), (vec![0], "xml")] {
            let (html, placed, _) = built_with(std::slice::from_ref(&siglum), &refused);
            assert_eq!(
                hrefs(&html),
                [format!("{base}.{extension}")],
                "{:?}",
                placed[0]
            );
            assert!(!html.contains("%2525"), "encoded twice");
        }
    }

    /// **A siglum of 255 bytes** is cut to fit with its extension, and its
    /// PDF takes the same cut: the link at the PDF has the base of the XML,
    /// and neither name is longer than a file name may be.
    #[test]
    fn a_siglum_of_255_bytes_links_a_pdf_of_the_same_base_as_its_xml() {
        let long = "K".repeat(255);
        let (html, placed, pdfs) =
            built_with(&[fragment(&long, "CTH 5", "root/CTH 5_XML_HFR/a.xml")], &[]);
        let xml = &placed[0].relative;
        let pdf = pdfs[0].built().expect("built");
        for name in [xml, pdf] {
            let file = name.file_name().expect("a name").len();
            assert!(file <= 255, "{file} bytes: {}", name.display());
        }
        assert_eq!(pdf.with_extension("xml"), *xml, "another base");
        assert_eq!(hrefs(&html), [crate::export::naming::href(pdf)]);
        assert!(
            html.contains(&format!(">{long}</a>")),
            "the siglum is cut in the name"
        );
    }

    /// The row's first cell is where [`links_by_row`] cuts, so it has to be
    /// in the row's markup once and in no other fragment the inventory is
    /// made of.
    #[test]
    fn only_a_manuscript_row_begins_with_its_number() {
        assert_eq!(
            include_str!("../generated/manuscript_row.html")
                .matches(ROW_START)
                .count(),
            1
        );
        for other in [
            include_str!("../generated/document.html"),
            include_str!("../generated/group_heading.html"),
            include_str!("../generated/column_heading.html"),
            include_str!("../generated/column_width.html"),
            include_str!("../generated/legend_entry.html"),
            include_str!("../generated/generated_line.html"),
            include_str!("../generated/manuscript_link.html"),
            include_str!("../generated/inventory_filter.js"),
        ] {
            assert!(!other.contains(ROW_START));
        }
    }
}
