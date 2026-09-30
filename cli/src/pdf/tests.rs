//! The ways a build of one PDF fails, each reached by the input that causes
//! it rather than by a real fault: a template that refuses, one that warns,
//! one that asks for a file, one that drops what the page predicted.

use std::path::Path;
use std::sync::OnceLock;

use super::layout::{Page, Piece};
use super::*;

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts");
        match Fonts::load(&dir) {
            Ok(f) => f,
            Err(e) => panic!("the fonts of the tree: {e}"),
        }
    })
}

fn page(line: &str) -> Page {
    Page {
        head: "CTH 0 – test".into(),
        name: "test".into(),
        sections: vec![vec![vec![Piece::Text(line.into())]]],
    }
}

/// The template's opening lines: the data, and nothing set.
const QUIET: &str = "#let d = json(\"/doc.json\")\n#set document(date: none)\n#set text(font: \"Noto Serif\", fallback: false)\n";

fn error(line: &str, template: &str) -> PdfError {
    match render_page(&page(line), "x", fonts(), template) {
        Ok(_) => panic!("the template {template:?} built"),
        Err(e) => e,
    }
}

#[test]
fn a_template_that_fails_refuses_the_document_and_only_it() {
    let e = error("a", &format!("{QUIET}#panic(\"no\")"));
    assert!(
        matches!(&e, PdfError::Document(m) if m.iter().any(|m| m.contains("no"))),
        "{e}"
    );
    assert!(!e.is_invariant());
    assert!(e.to_string().starts_with("Typst refused the document"));
}

#[test]
fn a_warning_refuses_the_document() {
    let e = error("a", &format!("{QUIET}#text(font: \"No Such Face\")[x]"));
    assert!(
        matches!(&e, PdfError::Document(m) if m[0].starts_with("warning:")),
        "{e}"
    );
}

#[test]
fn a_template_that_asks_for_another_file_stops_the_build() {
    let e = error("a", &format!("{QUIET}#image(\"/other.png\")"));
    assert_eq!(
        e.to_string(),
        PdfError::Invariant(Invariant::WorldAsked(vec!["other.png".into()])).to_string()
    );
    assert!(e.is_invariant());
}

#[test]
fn a_credit_predicted_and_not_embedded_stops_the_build() {
    let e = error("\u{100000}", &format!("{QUIET}x"));
    assert!(
        matches!(
            e,
            PdfError::Invariant(Invariant::Credit {
                predicted: true,
                embedded: false
            })
        ),
        "{e}"
    );
}

#[test]
fn a_label_with_no_image_stops_the_build() {
    let e = error("\u{100009}", &format!("{QUIET}x"));
    assert!(
        matches!(
            e,
            PdfError::Invariant(Invariant::LabelsUnplaced {
                in_text: 1,
                in_file: 0
            })
        ),
        "{e}"
    );
}

#[test]
fn the_page_as_the_template_sets_it() {
    let r = match render_page(&page("a-na \u{12000}\u{100009}"), "x", fonts(), TEMPLATE) {
        Ok(r) => r,
        Err(e) => panic!("{e}"),
    };
    assert_eq!(r.labels, [0x100009]);
    assert!(!r.credit);
    assert!(r.pdf.starts_with(b"%PDF-"));
}

#[test]
fn fonts_from_a_directory_that_is_not_the_tree_are_refused() {
    let Err(e) = Fonts::load(Path::new("/nonexistent/fonts")) else {
        panic!("fonts from nowhere");
    };
    assert!(matches!(e, PdfError::Fonts(_)), "{e}");
    assert!(e.is_invariant(), "no PDF can be built without the fonts");
}

#[test]
fn a_file_that_is_not_a_pdf_cannot_be_patched() {
    let label = post::Label {
        cp: 0x100009,
        height_em: 1.0,
        depth_share: 0.2,
        width_em: 3.0,
    };
    let Err(e) = post::finish(b"not a pdf", &[label], false) else {
        panic!("patched bytes that are not a PDF");
    };
    assert!(matches!(e, PdfError::Invariant(Invariant::Pdf(_))), "{e}");
}

#[test]
fn a_file_that_needs_no_patch_is_returned_as_it_came() {
    let (out, report) = match post::finish(b"any bytes", &[], false) {
        Ok(x) => x,
        Err(e) => panic!("{e}"),
    };
    assert_eq!(out, b"any bytes");
    assert_eq!((report.labels, report.clusters.len()), (0, 0));
}

#[test]
fn the_label_of_a_code_point_is_drawn_once_per_run() {
    let a = fonts().art(0x100005).map_err(|e| e.to_string());
    let b = fonts().art(0x100005).map_err(|e| e.to_string());
    assert_eq!(a, b);
    let a = a.unwrap_or_else(|e| panic!("{e}"));
    assert!(a.svg.starts_with("<svg") && a.width_em > a.height_em);
}

mod layout_rules {
    use super::super::layout::*;

    #[test]
    fn whitespace_is_one_space_kept_at_the_edges_of_a_piece() {
        assert_eq!(one_space(" a \t b\n"), "a b");
        let doc = "<r><lb/> a <note c=\" n  1 \"/> b <text:tab xmlns:text=\"t\"/>c<text:tab xmlns:text=\"t\"/>  </r>";
        let d = crate::document::Document::read(doc.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            sections(&d),
            [vec![vec![
                Piece::Text("a ".into()),
                Piece::Note("n 1".into()),
                Piece::Text(" b ".into()),
                Piece::Tab,
                Piece::Text("c".into()),
            ]]]
        );
    }

    #[test]
    fn a_line_of_tabs_alone_is_no_line() {
        let doc = "<r><lb/><text:tab xmlns:text=\"t\"/><text:tab xmlns:text=\"t\"/><lb/>x</r>";
        let d = crate::document::Document::read(doc.as_bytes()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(sections(&d), [vec![vec![Piece::Text("x".into())]]]);
    }

    #[test]
    fn json_escapes_what_json_has_to() {
        assert_eq!(json_str("a\"b\\c\u{1}"), "\"a\\\"b\\\\c\\u0001\"");
    }
}

#[test]
fn a_template_that_imports_a_file_stops_the_build() {
    let e = error("a", &format!("{QUIET}#import \"/other.typ\": *"));
    assert!(
        matches!(e, PdfError::Invariant(Invariant::WorldAsked(_))),
        "{e}"
    );
}

#[test]
fn the_world_has_no_today() {
    let e = error("a", &format!("{QUIET}#datetime.today().display()"));
    assert!(matches!(e, PdfError::Document(_)), "{e}");
}

#[test]
fn a_heading_takes_the_title_of_a_group_or_of_its_parent() {
    let doc = "<r><lb/>x</r>";
    let d = match crate::document::Document::read(doc.as_bytes()) {
        Ok(d) => d,
        Err(e) => panic!("{e}"),
    };
    let exact = Page::of(&d, "CTH 746", "x").head;
    let parent = Page::of(&d, "CTH 746.ZZ", "x").head;
    assert!(
        exact.starts_with("CTH 746 – ") && exact.len() > "CTH 746 – ".len(),
        "{exact}"
    );
    assert_eq!(parent, exact.replace("CTH 746", "CTH 746.ZZ"));
    assert_eq!(Page::of(&d, "no CTH", "x").head, "no CTH – ");
}

#[test]
fn an_error_of_the_pdf_writer_is_the_documents_refusal() {
    let e = refused(typst::ecow::EcoVec::new());
    assert!(
        matches!(e, PdfError::Document(ref m) if m.is_empty()),
        "{e}"
    );
}
