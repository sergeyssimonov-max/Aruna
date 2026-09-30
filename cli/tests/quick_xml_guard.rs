//! The guard of the two advisories excepted in `.cargo/audit.toml` and
//! `deny.toml`: RUSTSEC-2026-0194 and RUSTSEC-2026-0195 in `quick-xml` 0.38.4,
//! which reaches this crate through `typst-library` 0.15.1 → `hayagriva`
//! 0.10.1 → `citationberg` 0.7.0.
//!
//! The exception stands on one proof (specification 5.1, owner's decision of
//! 2026-09-27; carried to `main` by the decision of 2026-09-30, question 9 of
//! the design note): `citationberg` parses XML only when a bibliography or a
//! citation is handed a CSL style as a path or as bytes, and nothing here ever
//! hands one. These three tests are what fails if that stops being true:
//!
//! 1. no Typst template of this crate names a bibliography, a citation, a
//!    reference, a style, or a way to turn a string into code or to read a
//!    file – every `.typ` file under `cli/` is read, so the template of the
//!    PDF module is held by this test the day it lands;
//! 2. no Rust source of this crate outside this file names the crates or the
//!    types that parse CSL;
//! 3. a document whose text is Typst markup that would load a bibliography, a
//!    CSL style and other files is set as text word for word, and Typst asks
//!    the world for the template and the data and for nothing else.
//!
//! Test 3 runs the product's renderer, `aruna::pdf::render`, and its world –
//! `/main.typ` and `/doc.json`, the document as JSON data.

#[path = "support/pdf_check.rs"]
mod pdf_check;

use std::path::{Path, PathBuf};

use aruna::document::Document;
use aruna::pdf::{self, Fonts, Page};

/// The words that open a path to CSL, or to markup read from data, in a
/// template: a bibliography, a citation, a reference, a style file, and every
/// way to turn a string into code or to read another file.
const FORBIDDEN_IN_TEMPLATE: [&str; 12] = [
    "bibliography",
    "cite",
    "@",
    "csl",
    "eval",
    "import",
    "include",
    "read(",
    "xml(",
    "yaml(",
    "toml(",
    "cbor(",
];

/// The crates and types of the one path to `quick-xml` 0.38.4.
const CITATION_MACHINERY: [&str; 5] = [
    "hayagriva",
    "citationberg",
    "CslStyle",
    "CslSource",
    "ArchivedStyle",
];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every file under `dir` with the extension, `target` and `fixtures` aside.
fn files_with(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a readable source directory");
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "target" && name != "fixtures" {
                files_with(&path, extension, out);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

/// The template text a check reads: Typst comments and string literals of the
/// template are part of it on purpose – a forbidden word in a comment is
/// still a word someone meant to write.
#[test]
fn no_template_opens_a_path_to_a_csl_style() {
    let mut templates = Vec::new();
    files_with(&crate_dir(), "typ", &mut templates);
    for path in &templates {
        let template = std::fs::read_to_string(path)
            .expect("a template is text")
            .to_lowercase();
        for word in FORBIDDEN_IN_TEMPLATE {
            assert!(!template.contains(word), "{} says {word:?}", path.display());
        }
        assert!(
            template.matches("json(").count() <= 1,
            "{} loads more than one data file",
            path.display()
        );
    }
    let product = crate_dir().join("src").join("pdf").join("template.typ");
    assert!(
        templates.contains(&product),
        "the scan reaches the product's template"
    );
}

#[test]
fn no_source_calls_the_citation_machinery() {
    let mut sources = Vec::new();
    for dir in ["src", "tests", "examples", "benches"] {
        let dir = crate_dir().join(dir);
        if dir.is_dir() {
            files_with(&dir, "rs", &mut sources);
        }
    }
    let this = crate_dir().join("tests").join("quick_xml_guard.rs");
    assert!(sources.contains(&this), "the scan reaches this file");
    for path in sources.iter().filter(|p| **p != this) {
        let source = std::fs::read_to_string(path).expect("a source is text");
        for word in CITATION_MACHINERY {
            assert!(!source.contains(word), "{} names {word}", path.display());
        }
    }
}

/// Test 3 on the product's own renderer: a document whose text is markup
/// that would load a bibliography, a CSL style and other files.
#[test]
fn markup_in_a_document_is_set_as_text() {
    let lines = [
        "#bibliography(\"x.bib\")",
        "#bibliography(\"x.bib\", style: \"/doc.json\")",
        "@ref and #cite(&lt;k&gt;) and #ref(&lt;k&gt;)",
        "#import \"/doc.json\" #include \"/x.typ\" #read(\"/doc.json\")",
        "$x^2$ *bold* _it_ `raw` &lt;lab&gt; = heading - item + item / term: x",
    ];
    let body: String = lines.iter().map(|l| format!("<lb/> <w>{l}</w>")).collect();
    let xml = format!("<?xml version=\"1.0\"?><AOxml><body><text>{body}</text></body></AOxml>");
    let doc = Document::read(xml.as_bytes()).expect("a well-formed sample");
    let fonts = Fonts::load(&crate_dir().join("resources").join("fonts"))
        .expect("the fonts are the files docs/FONTS.md records");
    let page = Page::of(&doc, "CTH 0", "KBo 0.1");
    let r = pdf::render(&doc, "CTH 0", "KBo 0.1", "CTH 0/KBo 0.1.pdf", &fonts)
        .unwrap_or_else(|e| panic!("the document is refused: {e}"));
    assert_eq!(
        r.asked.iter().map(String::as_str).collect::<Vec<_>>(),
        ["doc.json", "main.typ"],
        "the world was asked for more than the template and the data"
    );
    let want = pdf_check::cps(&pdf_check::expected_text(&page));
    let tree = pdf_check::export_tree(&r.pdf).expect("tagged");
    let got = pdf_check::cps(&pdf_check::tree_text(&tree.xml));
    assert_eq!(
        pdf_check::first_difference(&want, &got),
        None,
        "the text of the document came out changed"
    );
    for line in lines {
        let line = line.replace("&lt;", "<").replace("&gt;", ">");
        assert!(
            page.texts().any(|t| t == line),
            "{line:?} is not a line of the page"
        );
    }
}
