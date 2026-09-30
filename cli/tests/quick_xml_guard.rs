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
//! Test 3 builds its own minimal world, the shape of the one the trial proved
//! (`/main.typ` and `/doc.json`, the document as JSON data): until the PDF
//! module exists it guards the mechanism, not the product. When
//! `aruna::pdf` lands it is re-pointed at the module's own renderer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::layout::{Frame, FrameItem};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

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
    // The inline template of test 3 is held to the same words.
    let inline = TEMPLATE.to_lowercase();
    for word in FORBIDDEN_IN_TEMPLATE {
        assert!(!inline.contains(word), "the test's template says {word:?}");
    }
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

/// The template of test 3: the document arrives as data and is set as text.
const TEMPLATE: &str = r#"#let d = json("/doc.json")
#set document(date: none)
#set text(font: "Noto Serif", fallback: false)
#for line in d.lines [#line \ ]
"#;

struct Guard {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    main_id: FileId,
    data_id: FileId,
    main: Source,
    data: Bytes,
    /// Every path Typst asked the world for, served or not.
    asked: Mutex<BTreeSet<String>>,
}

fn file_id(p: &str) -> FileId {
    RootedPath::new(VirtualRoot::Project, VirtualPath::new(p).expect("a path")).intern()
}

impl Guard {
    fn ask(&self, id: FileId) {
        self.asked
            .lock()
            .expect("an unpoisoned log")
            .insert(format!("{:?}", id.vpath()));
    }
}

impl World for Guard {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.main_id
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        self.ask(id);
        if id == self.main_id {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from(
                "only the template exists",
            )))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.ask(id);
        if id == self.data_id {
            Ok(self.data.clone())
        } else {
            Err(FileError::NotFound(PathBuf::from("only doc.json exists")))
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn text_of(frame: &Frame, out: &mut String) {
    for (_, item) in frame.items() {
        match item {
            FrameItem::Group(group) => text_of(&group.frame, out),
            FrameItem::Text(text) => out.push_str(&text.text),
            _ => {}
        }
    }
}

#[test]
fn markup_in_a_document_is_set_as_text() {
    let lines = [
        "#bibliography(\"x.bib\")",
        "#bibliography(\"x.bib\", style: \"/doc.json\")",
        "@ref and #cite(<k>) and #ref(<k>)",
        "#import \"/doc.json\" #include \"/x.typ\" #read(\"/doc.json\")",
        "$x^2$ *bold* _it_ `raw` <lab> = heading - item + item / term: x",
    ];
    let dir = crate_dir().join("resources").join("fonts");
    aruna::fonts::verify_dir(&dir).expect("the fonts are the files docs/FONTS.md records");
    let data = std::fs::read(dir.join("NotoSerif-Regular.ttf")).expect("the main face");
    let fonts: Vec<Font> = Font::iter(Bytes::new(data)).collect();
    let main_id = file_id("/main.typ");
    let json = format!(
        "{{\"lines\":[{}]}}",
        lines
            .iter()
            .map(|l| json_str(l))
            .collect::<Vec<_>>()
            .join(",")
    );
    let world = Guard {
        library: LazyHash::new(Library::default()),
        book: LazyHash::new(FontBook::from_fonts(&fonts)),
        fonts,
        main_id,
        data_id: file_id("/doc.json"),
        main: Source::new(main_id, TEMPLATE.to_string()),
        data: Bytes::new(json.into_bytes()),
        asked: Mutex::new(BTreeSet::new()),
    };
    let warned = typst::compile::<PagedDocument>(&world);
    let warnings: Vec<String> = warned
        .warnings
        .iter()
        .map(|w| w.message.to_string())
        .collect();
    assert!(warnings.is_empty(), "{warnings:?}");
    let document = warned.output.unwrap_or_else(|errors| {
        panic!(
            "{:?}",
            errors
                .iter()
                .map(|e| e.message.to_string())
                .collect::<Vec<_>>()
        )
    });
    let asked = world.asked.lock().expect("an unpoisoned log").clone();
    assert_eq!(
        asked,
        ["\"/doc.json\"", "\"/main.typ\""]
            .into_iter()
            .map(String::from)
            .collect::<BTreeSet<_>>(),
        "the world was asked for {asked:?}"
    );
    let mut set = String::new();
    for page in document.pages() {
        text_of(&page.frame, &mut set);
    }
    let want: String = lines.concat().chars().filter(|c| *c != ' ').collect();
    let got: String = set.chars().filter(|c| *c != ' ').collect();
    assert_eq!(got, want, "the text of the document came out changed");
}
