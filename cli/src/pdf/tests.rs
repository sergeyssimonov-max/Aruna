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
fn successive_document_worlds_share_the_parsed_template() {
    use typst::World;

    let template = Template::new(TEMPLATE);
    let main_source = |json: &str| {
        let world = world::PdfWorld::new(fonts(), &template, json.to_owned()).expect("world");
        world.source(world.main()).expect("main.typ")
    };
    let first = main_source(r#"{"name":"A"}"#);
    let second = main_source(r#"{"name":"B"}"#);
    assert!(std::ptr::eq(first.root(), second.root()));
}

#[test]
fn a_shared_template_recovers_after_refusing_one_document() {
    let text = format!("{TEMPLATE}\n#if d.plain-name == \"B\" {{ panic(\"B refused\") }}");
    let template = Template::new(&text);
    let doc = Document::read(b"<r><lb/>a-na</r>").expect("well-formed text");
    let render = |name| render_with_template(&doc, "CTH 1", name, name, fonts(), &template);

    let first = render("A").expect("A builds");
    let refusal = render("B").expect_err("Typst refuses B");
    assert!(matches!(refusal, PdfError::Document(ref messages)
        if messages.iter().any(|message| message.contains("B refused"))));
    let again = render("A").expect("A still builds");
    assert!(first.pdf == again.pdf, "the refusal changed A's PDF");
}

fn render_manuscript(
    doc: &Document<'_>,
    name: &str,
    template: &Template<'_>,
) -> Result<Rendered, PdfError> {
    let ident = format!("CTH 746/{name}.pdf");
    render_with_template(doc, "CTH 746", name, &ident, fonts(), template)
}

#[test]
fn alternating_templates_preserves_each_manuscripts_pdf() {
    let a = Document::read(
        concat!(
            "<r><lb cu=\"\u{12000}\u{100009}\u{12000}\"/> <w>a\u{100009}b</w>",
            "<note c=\"a note\"/> <w>MEŠ\u{0303}</w></r>"
        )
        .as_bytes(),
    )
    .expect("manuscript with labels");
    let b = Document::read("<r><lb/> <w>nu</w> <w>zi-ik</w> <lb/> <w>ta-ma-iš</w></r>".as_bytes())
        .expect("manuscript without a label");
    let standard = Template::new(TEMPLATE);
    let wider_text = TEMPLATE.replace("margin: 2cm", "margin: 3cm");
    assert_ne!(
        wider_text, TEMPLATE,
        "the alternate margins must take effect"
    );
    let wider = Template::new(&wider_text);

    let first = render_manuscript(&a, "KBo 52.182", &standard).expect("A builds");
    let between = render_manuscript(&b, "KUB 1.1", &wider).expect("B builds");
    let again = render_manuscript(&a, "KBo 52.182", &standard).expect("A builds again");
    assert!(first.pdf == again.pdf, "B changed the next PDF of A");
    assert_eq!(first.labels, [0x100009, 0x100009]);
    assert_eq!(again.labels, first.labels);
    assert!(between.labels.is_empty());
    assert_eq!(first.asked, between.asked);
    assert_eq!(first.asked, again.asked);

    let page_text = |rendered: &Rendered| {
        let pdf = lopdf::Document::load_mem(&rendered.pdf).expect("readable PDF");
        let page = pdf.page_iter().next().expect("first page");
        post::scan_page(&pdf, page)
            .expect("page text")
            .text
            .replace(char::is_whitespace, "")
    };
    let text_a = page_text(&again);
    let text_b = page_text(&between);
    assert!(
        text_a.contains("KBo52.182") && !text_a.contains("KUB1.1"),
        "{text_a}"
    );
    assert!(
        text_b.contains("KUB1.1") && !text_b.contains("KBo52.182"),
        "{text_b}"
    );
    assert!(
        text_a.contains("a\u{100009}b") && !text_a.contains("ta-ma-iš"),
        "{text_a}"
    );
    assert!(
        text_b.contains("ta-ma-iš") && !text_b.contains("a\u{100009}b"),
        "{text_b}"
    );

    // Keep the manuscript and its PDF identifier fixed while changing only the margins.
    let plain_b = render_manuscript(&b, "KUB 1.1", &standard).expect("B with standard margins");
    let wider_a = render_manuscript(&a, "KBo 52.182", &wider).expect("A with wider margins");
    assert!(plain_b.pdf != between.pdf, "B ignored its template");
    assert!(wider_a.pdf != first.pdf, "A ignored its template");

    let fresh = render(
        &a,
        "CTH 746",
        "KBo 52.182",
        "CTH 746/KBo 52.182.pdf",
        fonts(),
    )
    .expect("A builds with a fresh template");
    assert!(fresh.pdf == first.pdf, "reusing the template changed A");
}

#[test]
fn a_cluster_refusal_leaves_the_next_pdf_unchanged() {
    let good = Document::read("<r><lb/>nu MEŠ\u{0303}</r>".as_bytes()).expect("one cluster");
    let overlap =
        Document::read("<r><lb/>Š\u{0303}Š\u{0303}</r>".as_bytes()).expect("two adjacent clusters");
    let template = Template::new(TEMPLATE);

    let before = render_manuscript(&good, "KBo 0.2", &template).expect("good manuscript builds");
    let Err(refusal) = render_manuscript(&overlap, "KBo 0.1", &template) else {
        panic!("the overlapping clusters were accepted");
    };
    assert!(matches!(refusal, PdfError::Clusters { .. }), "{refusal}");
    assert!(!refusal.is_invariant());
    let after =
        render_manuscript(&good, "KBo 0.2", &template).expect("good manuscript still builds");
    assert_eq!(after.clusters, 1, "the good cluster is still wrapped");
    assert!(
        after.pdf == before.pdf,
        "the cluster refusal changed the next PDF"
    );
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
                Piece::Note(vec![vec![Piece::Text("n 1".into())]]),
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
        assert_eq!(
            crate::json::json_string("a\"b\\c\u{1}"),
            "\"a\\\"b\\\\c\\u0001\""
        );
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
