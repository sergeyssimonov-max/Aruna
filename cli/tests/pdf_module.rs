//! The PDF of one document, checked the way the gate checks the whole set
//! (`PDF-ACCEPTANCE.md`, specification 6.9), on samples written here.
//!
//! The samples are synthetic, as every fixture of this repository is
//! (`cli/fixtures/xml/MANIFEST.md`): each reproduces one construct of the
//! corpus and names the document that has it. The same constructs on the
//! corpus itself are the heavy tests at the end, run with `ARUNA_ZIP`.

#[path = "support/pdf_check.rs"]
mod pdf_check;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aruna::document::Document;
use aruna::pdf::{self, Fonts, Page, Rendered};
use pdf_check::*;

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts");
        Fonts::load(&dir).expect("the fonts are the files docs/FONTS.md records")
    })
}

/// A document of the corpus's shape: a header, then the body the model sets.
fn doc(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<AOxml xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\"><AOHeader><docID>KBo 0.1</docID></AOHeader><body><text>{body}</text></body></AOxml>"
    )
}

fn build(xml: &str, name: &str) -> (Page, Rendered) {
    let d = Document::read(xml.as_bytes()).expect("a well-formed sample");
    let page = Page::of(&d, "CTH 746", name);
    let rendered = pdf::render(&d, "CTH 746", name, &format!("CTH 746/{name}.pdf"), fonts())
        .expect("the sample builds");
    (page, rendered)
}

/// Every check of the gate that one PDF can be put to.
fn assert_whole(page: &Page, r: &Rendered) -> Checked {
    let c = check(&r.pdf).expect("the reader reads it");
    assert!(c.outside.is_empty(), "outside the margin: {:?}", c.outside);
    assert_eq!(c.notdef, 0, "a blank glyph");
    assert!(
        c.credit_ok(),
        "credit {:?}, UllikummiA {}",
        c.credit_pages,
        c.ullikummi_embedded
    );
    assert_eq!(c.ullikummi_embedded, r.credit);
    assert!(
        c.same_composition(page),
        "the stream is not the source's code points"
    );
    assert_eq!(c.label_letters, 0, "a label's letters are text");
    let want = expected_tree(fonts(), page, r.credit);
    let got = export_tree(&r.pdf).expect("tagged");
    assert_eq!(tree_difference(&want, &got.xml), None, "{}", got.xml);
    assert_eq!((got.orphans, got.twice), (0, 0));
    c
}

/// The sample of a label: `U+100009`, a code point no face draws, beside
/// cuneiform (`CTH 670/KBo 52.182`).
const LABEL: &str = "<lb cu=\"\u{12000}\u{100009}\u{12000}\"/> <w>a\u{100009}b</w>";

#[test]
fn a_label_keeps_its_code_point_and_not_its_letters() {
    assert_eq!(fonts().face(0x100009), None, "U+100009 has a glyph now");
    let (page, r) = build(&doc(LABEL), "KBo 52.182");
    assert_eq!(r.labels, [0x100009, 0x100009]);
    let c = assert_whole(&page, &r);
    assert_eq!(
        c.raw.matches('\u{100009}').count(),
        2,
        "the invisible layer"
    );
    assert!(!c.raw.contains("U+"), "the letters are paths");
}

/// A cluster Typst 0.15.1 reads twice – `Š` and U+0303, as in
/// `CTH 678/DAAM 3.59` – is wrapped once and read once.
#[test]
fn a_repeated_cluster_is_wrapped_and_read_once() {
    let (page, r) = build(&doc("<lb/> <w>x ME\u{0160}\u{0303} x</w>"), "DAAM 3.59");
    assert_eq!(r.clusters, 1);
    let c = assert_whole(&page, &r);
    // The wrapper's ActualText is the text of the piece before the mark,
    // which ends in the cluster.
    assert_eq!(c.actual_texts.len(), 1, "{:?}", c.actual_texts);
    assert!(
        c.actual_texts.keys().all(|k| k.ends_with("U+0160 U+0303")),
        "{:?}",
        c.actual_texts
    );
    let want = cps(&expected_text(&page));
    let got = cps(&tree_text(&export_tree(&r.pdf).expect("tagged").xml));
    assert_eq!(first_difference(&want, &got), None);
}

/// Where no cluster repeats, no wrapper is put and Typst's file is left as
/// it is: a plain document carries no `/ActualText` at all.
#[test]
fn a_plain_document_is_not_patched() {
    let (page, r) = build(&doc("<lb/> <w>a-na</w> <w>be-li</w>"), "KBo 0.1");
    assert_eq!((r.clusters, r.labels.len(), r.credit), (0, 0, false));
    let c = assert_whole(&page, &r);
    assert!(c.actual_texts.is_empty(), "{:?}", c.actual_texts);
}

/// The credit stands once, on page one, in a file that uses UllikummiA –
/// here on a later page, as `U+100000` stands on the second page of
/// `CTH 569/KBo 54.99+` – and in no other file.
#[test]
fn the_credit_stands_once_on_the_first_page() {
    let mut body = String::new();
    for n in 0..120 {
        body.push_str(&format!("<lb/> <w>line {n} a-na be-li-ia</w>"));
    }
    body.push_str("<lb/> <w>\u{100000}</w>");
    let (page, r) = build(&doc(&body), "KBo 54.99+");
    assert!(r.credit);
    let c = assert_whole(&page, &r);
    assert!(c.pages > 1, "the sign is on a later page");
    assert_eq!(c.credit_pages.iter().sum::<usize>(), 1);
    assert_eq!(c.credit_pages[0], 1);
    let (page, r) = build(&doc("<lb/> <w>\u{12000}</w>"), "KBo 0.1");
    assert!(!r.credit);
    assert_eq!(assert_whole(&page, &r).credit_pages, [0]);
}

/// `text:tab` shifts the line by 2 em and adds nothing to the text: after
/// five tabs the text stands 100 pt in, as in `CTH 746/KUB 28.15`.
#[test]
fn a_tab_is_a_shift_and_not_a_character() {
    let xml = doc(
        "<lb/> <w><text:tab/><text:tab/><text:tab/><text:tab/><text:tab/>-un</w> <lb/> <w>word</w><text:tab/>",
    );
    let (page, r) = build(&xml, "KUB 28.15");
    assert!(page.sections[0]
        .iter()
        .any(|l| l.contains(&aruna::pdf::layout::Piece::Tab)));
    let c = assert_whole(&page, &r);
    let scan = &scan_pdf(&r.pdf).expect("pages")[0];
    let at = |t: &str| {
        scan.glyphs
            .iter()
            .find(|g| g.text == t)
            .map(|g| g.x0 - MARGIN)
            .unwrap_or_else(|| {
                panic!(
                    "{t:?} is not among {:?}",
                    scan.glyphs.iter().map(|g| &g.text).collect::<Vec<_>>()
                )
            })
    };
    let shift = at("-");
    let expected = 5.0 * pdf::TAB_EM * 10.0;
    assert!(
        (shift - expected).abs() < 0.5,
        "shifted {shift} pt, not {expected}"
    );
    assert!(at("w").abs() < 0.5, "a trailing tab shifts nothing");
    assert!(!c.raw.contains('\t'));
}

/// Comments are not set (owner's decision of 2026-09-30): the three of the
/// corpus stand in `AOHeader/meta/neu`, as in `CTH 627/IBoT 4.193`.
#[test]
fn a_comment_is_left_out() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<AOxml><AOHeader><meta><neu><!-- NN::/an editor's note --></neu></meta></AOHeader><body><text><lb/> <w>a-na</w></text></body></AOxml>";
    let (page, r) = build(xml, "IBoT 4.193");
    let c = assert_whole(&page, &r);
    assert!(!c.raw.contains("NN::"), "{}", c.raw);
    assert!(!export_tree(&r.pdf).expect("tagged").xml.contains("NN::"));
}

/// No date, no run identifier: the file names its path, the program and the
/// template, and two builds are one file.
#[test]
fn the_file_carries_no_date_and_builds_the_same_twice() {
    let xml = doc(&format!(
        "{LABEL}<note c=\"a note\"/> <w>ME\u{0160}\u{0303}</w>"
    ));
    let (_, a) = build(&xml, "KBo 52.182");
    let (_, b) = build(&xml, "KBo 52.182");
    assert!(a.pdf == b.pdf, "two builds differ");
    for word in [
        b"CreationDate".as_slice(),
        b"ModDate",
        b"CreateDate",
        b"ModifyDate",
        b"MetadataDate",
    ] {
        assert!(
            memchr::memmem::find(&a.pdf, word).is_none(),
            "{}",
            String::from_utf8_lossy(word)
        );
    }
    let doc = lopdf::Document::load_mem(&a.pdf).expect("readable");
    let info = doc
        .trailer
        .get(b"Info")
        .and_then(|o| o.as_reference())
        .and_then(|id| doc.get_dictionary(id))
        .expect("an Info dictionary");
    let field = |k: &[u8]| {
        info.get(k)
            .and_then(|o| o.as_str())
            .map(aruna::pdf::post::decode_text_string)
            .unwrap_or_default()
    };
    assert_eq!(
        field(b"Creator"),
        format!("Aruna {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(field(b"Keywords"), pdf::TEMPLATE_VERSION);
}

/// Reading 1 of the structural criterion, and its four negative controls:
/// each spoils the file one way and each must break the equality.
#[test]
fn the_tag_tree_is_the_models_and_every_control_breaks_it() {
    let xml = doc(&format!(
        "{LABEL}<note c=\"a note\"/> <w>tu</w><parsep/><lb/> <w>b</w> <lb/> <w>c</w>"
    ));
    let (page, r) = build(&xml, "KBo 52.182");
    assert_whole(&page, &r);
    let want = expected_tree(fonts(), &page, r.credit);
    let spoiled = |p: Page| {
        let r = pdf::render_page(&p, "x", fonts(), pdf::TEMPLATE).expect("builds");
        export_tree(&r.pdf).expect("tagged").xml
    };
    for (control, got) in [
        ("swap", spoiled(swap_lines(&page).expect("two lines"))),
        ("drop", spoiled(drop_note(&page).expect("a note"))),
        ("repeat", spoiled(repeat_line(&page).expect("a line"))),
        (
            "label",
            export_tree(&spoil_label(&r.pdf).expect("a label"))
                .expect("tagged")
                .xml,
        ),
    ] {
        assert!(
            tree_difference(&want, &got).is_some(),
            "control {control} goes unseen"
        );
    }
}

// ---------------------------------------------------------------------------
// The same constructs on the corpus: heavy, with ARUNA_ZIP
// ---------------------------------------------------------------------------

fn corpus_document(rel_in_zip_suffix: &str) -> Option<(String, Vec<u8>)> {
    let zip = std::env::var_os("ARUNA_ZIP").map(PathBuf::from)?;
    let file = std::fs::File::open(zip).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).ok()?;
        if entry.name().ends_with(rel_in_zip_suffix) {
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut bytes).ok()?;
            return Some((entry.name().to_string(), bytes));
        }
    }
    None
}

/// One document of each construct, from the archive: a label
/// (`KBo 52.182`), a repeated cluster (`CHDS 5.69`), the credit
/// (`KBo 54.99+`), `text:tab` (`KUB 28.15`), a comment (`IBoT 4.193`) and
/// the one with the most cuneiform (`KUB 5.1+`) – each passes every check.
#[test]
#[ignore = "needs the corpus archive: ARUNA_ZIP"]
fn the_samples_of_the_corpus_pass_every_check() {
    for (group, name) in [
        ("CTH 670", "KBo 52.182"),
        ("CTH 212", "CHDS 5.69"),
        ("CTH 569", "KBo 54.99+"),
        ("CTH 746", "KUB 28.15"),
        ("CTH 627", "IBoT 4.193"),
        ("CTH 561", "KUB 5.1+"),
    ] {
        let (entry, bytes) = corpus_document(&format!("/{name}.xml"))
            .unwrap_or_else(|| panic!("{name} is in the archive named by ARUNA_ZIP"));
        let d = Document::read(&bytes).unwrap_or_else(|e| panic!("{entry}: {e}"));
        let page = Page::of(&d, group, name);
        let r = pdf::render(&d, group, name, &format!("{group}/{name}.pdf"), fonts())
            .unwrap_or_else(|e| panic!("{entry}: {e}"));
        let c = assert_whole(&page, &r);
        let tabs = page.sections.iter().flatten().flatten();
        match name {
            "KBo 52.182" => assert!(r.labels.contains(&0x100009), "{name}: no label"),
            "CHDS 5.69" => assert_eq!(r.clusters, 1, "{name}"),
            "KBo 54.99+" => assert!(r.credit, "{name}: no credit"),
            "KUB 28.15" => assert!(
                tabs.filter(|p| **p == aruna::pdf::layout::Piece::Tab)
                    .count()
                    > 0,
                "{name}: no tab"
            ),
            "IBoT 4.193" => assert!(!c.raw.contains("NN::"), "{name}: the comment is set"),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Golden files of structure (specification 6.9): insta
// ---------------------------------------------------------------------------

/// What a PDF is made of, as text: pages and their sizes, the Info fields,
/// the fonts each page embeds. A layout regression or a lost font moves it;
/// a changed pixel does not.
fn structure(pdf: &[u8]) -> String {
    use std::fmt::Write as _;
    let doc = lopdf::Document::load_mem(pdf).expect("readable");
    let mut out = String::new();
    if let Ok(info) = doc
        .trailer
        .get(b"Info")
        .and_then(|o| o.as_reference())
        .and_then(|id| doc.get_dictionary(id))
    {
        for (k, v) in info.iter() {
            let v = v
                .as_str()
                .map(aruna::pdf::post::decode_text_string)
                .unwrap_or_else(|_| format!("{v:?}"));
            // The version moves with every release and is checked apart.
            let v = v.replace(env!("CARGO_PKG_VERSION"), "{version}");
            let _ = writeln!(out, "info {}: {v}", String::from_utf8_lossy(k));
        }
    }
    for (n, page) in doc.get_pages() {
        let scan = aruna::pdf::post::scan_page(&doc, page).expect("a page");
        // The Type3 font of the invisible layer has no BaseFont.
        let mut fonts: Vec<String> = scan
            .fonts
            .iter()
            .map(|f| {
                if f.is_empty() {
                    "(Type3 of the labels)".into()
                } else {
                    f.clone()
                }
            })
            .collect();
        fonts.sort();
        let _ = writeln!(
            out,
            "page {n}: {:.2} × {:.2} pt, {} glyphs, fonts {}",
            scan.width,
            scan.height,
            scan.glyphs.len(),
            fonts.join(", ")
        );
    }
    out
}

#[test]
fn the_structure_of_each_sample_is_its_snapshot() {
    for (name, body) in [
        ("label", LABEL.to_string()),
        ("cluster", "<lb/> <w>x ME\u{0160}\u{0303} x</w>".to_string()),
        (
            "credit",
            "<lb/> <w>\u{100000} a-na</w> <note c=\"a note\"/>".to_string(),
        ),
        (
            "tab",
            "<lb/> <w><text:tab/><text:tab/>-un</w><parsep/><lb/> <w>b</w>".to_string(),
        ),
    ] {
        let (_, r) = build(&doc(&body), name);
        insta::assert_snapshot!(format!("structure-{name}"), structure(&r.pdf));
    }
}

/// **The reader's teeth**: a label whose invisible text is spoiled, and a
/// label layer that lost its `ToUnicode`, both fail the check a good file
/// passes – the same probes the gate puts to PDFKit.
#[test]
fn a_spoiled_label_or_a_lost_to_unicode_fails_the_reader() {
    let (_, r) = build(&doc(LABEL), "KBo 52.182");
    let count = |pdf: &[u8]| {
        check(pdf)
            .expect("readable")
            .raw
            .matches('\u{100009}')
            .count()
    };
    assert_eq!(count(&r.pdf), 2);
    assert_ne!(count(&spoil_label(&r.pdf).expect("a label")), 2);
    assert_eq!(count(&drop_label_to_unicode(&r.pdf).expect("a layer")), 0);
}
