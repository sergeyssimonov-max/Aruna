//! The document model, against the corpus and against a reader this project
//! did not write.
//!
//! Three questions, each on the real archive:
//!
//! - **complete** — for a sample of documents, the model says exactly what
//!   `xsltproc` says the document contains: every element, attribute, text,
//!   comment and instruction, nothing more;
//! - **in order** — the same comparison is line for line, so a node in the wrong
//!   place fails it as surely as a missing one;
//! - **stable** — over the whole corpus, twice: no panic, the same model both
//!   times, and a refusal for exactly the documents the manifest already names.
//!
//! **Why `xsltproc`.** `docs/PDF-ACCEPTANCE.md` §1–2 names it as the independent
//! extraction: it ships with macOS, it is libxml2 underneath rather than
//! `quick-xml`, and what it reports is the XML Information Set — which is what
//! the model claims to be. Comparing the model against the model's own reading
//! would measure this project against itself.
//!
//! Skipped when the archive is not there, as `tests/corpus.rs` is, and turned
//! into a failure by `ARUNA_REQUIRE_FIXTURE=1`. The comparisons with `xsltproc`
//! are skipped when it is not installed; its absence is never read as a pass —
//! the test says so and does nothing.
//!
//! ```sh
//! cargo nextest run --locked --test document_model
//! # the whole corpus against xsltproc — minutes:
//! cargo nextest run --locked --test document_model --run-ignored all
//! ```

use aruna::document::{Document, Kind, Refusal};
use aruna::md5::md5_hex;
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, HEADER_READ_LIMIT};
use aruna::xml_wellformed::{beyond_the_parser, classify};
use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The archive, wherever this run keeps it — the same three places
/// `tests/corpus.rs` looks, in the same order.
fn archive() -> Option<PathBuf> {
    for name in ["ARUNA_ZIP", "ARUNA_FIXTURE_ZIP"] {
        if let Some(named) = std::env::var_os(name) {
            return Some(PathBuf::from(named));
        }
    }
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
    default.is_file().then_some(default)
}

fn required() -> Option<PathBuf> {
    match archive() {
        Some(path) if path.is_file() => Some(path),
        other => {
            assert!(
                std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
                "ARUNA_REQUIRE_FIXTURE is set but {other:?} is missing"
            );
            eprintln!("skipping: the corpus archive is not present");
            None
        }
    }
}

fn xsltproc_present() -> bool {
    let present = Command::new("xsltproc")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !present {
        eprintln!("xsltproc is not installed: the comparison did not run, and nothing is claimed");
    }
    present
}

/// Every document the gates admit, with its path inside the archive (the
/// archive's own top folder stripped), handed over one at a time.
fn each_document(path: &Path, mut visit: impl FnMut(&str, &[u8])) -> usize {
    let file = std::fs::File::open(path).expect("open the archive");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read the archive");
    let mut bytes = Vec::new();
    let mut admitted = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        if !is_manuscript_xml(entry.name()) {
            continue;
        }
        let name = entry
            .name()
            .split_once('/')
            .map_or(entry.name(), |(_, rest)| rest)
            .to_string();
        bytes.clear();
        entry.read_to_end(&mut bytes).expect("read the document");
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        admitted += 1;
        visit(&name, &bytes);
    }
    admitted
}

// ---------------------------------------------------------------------------
// One listing, two producers
// ---------------------------------------------------------------------------

/// The listing, as `xsltproc` produces it from the document.
///
/// One line per node in document order. Values are written with their length
/// in characters before them, so text containing a line feed cannot be taken
/// for the next line and nothing needs escaping. `string-length` counts
/// characters, and the model's side counts `chars()`.
///
/// `N` is the number of namespaces in scope on an element, the `xml` one
/// included; `B` lists them for the root. The namespace axis has no defined
/// order, so runs of `B` lines are sorted on both sides before comparing.
/// Attribute order on `@*` is not defined by XPath either; libxml2 gives the
/// order of the document, and that is the order compared.
const LISTING: &str = r#"<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
<xsl:output method="text" encoding="UTF-8"/>
<xsl:template match="/"><xsl:apply-templates select="node()"/></xsl:template>
<xsl:template match="*">
<xsl:text>E {</xsl:text><xsl:value-of select="namespace-uri()"/><xsl:text>}</xsl:text><xsl:value-of select="local-name()"/><xsl:text>&#10;</xsl:text>
<xsl:text>N </xsl:text><xsl:value-of select="count(namespace::*)"/><xsl:text>&#10;</xsl:text>
<xsl:if test="not(parent::*)"><xsl:for-each select="namespace::*"><xsl:text>B </xsl:text><xsl:value-of select="name()"/><xsl:text> </xsl:text><xsl:value-of select="."/><xsl:text>&#10;</xsl:text></xsl:for-each></xsl:if>
<xsl:for-each select="@*"><xsl:text>A {</xsl:text><xsl:value-of select="namespace-uri()"/><xsl:text>}</xsl:text><xsl:value-of select="local-name()"/><xsl:text> </xsl:text><xsl:value-of select="string-length(.)"/><xsl:text>:</xsl:text><xsl:value-of select="."/><xsl:text>&#10;</xsl:text></xsl:for-each>
<xsl:apply-templates select="node()"/>
<xsl:text>/E&#10;</xsl:text>
</xsl:template>
<xsl:template match="text()"><xsl:text>T </xsl:text><xsl:value-of select="string-length(.)"/><xsl:text>:</xsl:text><xsl:value-of select="."/><xsl:text>&#10;</xsl:text></xsl:template>
<xsl:template match="comment()"><xsl:text>C </xsl:text><xsl:value-of select="string-length(.)"/><xsl:text>:</xsl:text><xsl:value-of select="."/><xsl:text>&#10;</xsl:text></xsl:template>
<xsl:template match="processing-instruction()"><xsl:text>P </xsl:text><xsl:value-of select="name()"/><xsl:text> </xsl:text><xsl:value-of select="string-length(.)"/><xsl:text>:</xsl:text><xsl:value-of select="."/><xsl:text>&#10;</xsl:text></xsl:template>
</xsl:stylesheet>
"#;

/// The same listing, from the model.
fn listing(document: &Document<'_>) -> String {
    let nodes = document.nodes();
    let mut out = String::new();
    let mut open: Vec<usize> = Vec::new();
    let value = |out: &mut String, text: &str| {
        out.push_str(&text.chars().count().to_string());
        out.push(':');
        out.push_str(text);
        out.push('\n');
    };
    for (at, node) in nodes.iter().enumerate() {
        while open.last().is_some_and(|&element| nodes[element].end <= at) {
            open.pop();
            out.push_str("/E\n");
        }
        match &node.kind {
            Kind::Element(element) => {
                out.push_str("E {");
                out.push_str(element.name.namespace.as_deref().unwrap_or(""));
                out.push('}');
                out.push_str(&element.name.local);
                out.push('\n');
                let scope = in_scope(document, at);
                out.push_str(&format!("N {}\n", scope.len()));
                if node.parent.is_none() {
                    for (prefix, uri) in &scope {
                        out.push_str(&format!("B {prefix} {uri}\n"));
                    }
                }
                for attribute in &element.attributes {
                    out.push_str("A {");
                    out.push_str(attribute.name.namespace.as_deref().unwrap_or(""));
                    out.push('}');
                    out.push_str(&attribute.name.local);
                    out.push(' ');
                    value(&mut out, &attribute.value);
                }
                open.push(at);
            }
            Kind::Text(text) => {
                out.push_str("T ");
                value(&mut out, text);
            }
            Kind::Comment(text) => {
                out.push_str("C ");
                value(&mut out, text);
            }
            Kind::Instruction { target, data } => {
                out.push_str("P ");
                out.push_str(target);
                out.push(' ');
                value(&mut out, data);
            }
        }
    }
    for _ in open {
        out.push_str("/E\n");
    }
    out
}

/// Namespaces in scope on the element at `at`: every declaration on it and its
/// ancestors, the nearest winning, `xmlns=""` taking the default away, and the
/// `xml` prefix always.
fn in_scope(document: &Document<'_>, at: usize) -> BTreeMap<String, String> {
    let nodes = document.nodes();
    let mut chain = Vec::new();
    let mut next = Some(at);
    while let Some(index) = next {
        chain.push(index);
        next = nodes[index].parent;
    }
    let mut scope = BTreeMap::new();
    scope.insert(
        "xml".to_string(),
        "http://www.w3.org/XML/1998/namespace".to_string(),
    );
    for &index in chain.iter().rev() {
        if let Kind::Element(element) = &nodes[index].kind {
            for binding in &element.namespaces {
                let prefix = binding.prefix.as_deref().unwrap_or("").to_string();
                if binding.uri.is_empty() {
                    scope.remove(&prefix);
                } else {
                    scope.insert(prefix, binding.uri.to_string());
                }
            }
        }
    }
    scope
}

/// Runs of `B` lines sorted, everything else as it stands.
fn comparable(listing: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    for line in listing.split_inclusive('\n') {
        if line.starts_with("B ") {
            run.push(line.to_string());
            continue;
        }
        run.sort();
        lines.append(&mut run);
        lines.push(line.to_string());
    }
    run.sort();
    lines.append(&mut run);
    lines
}

struct Xsltproc {
    dir: tempfile::TempDir,
}

impl Xsltproc {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("listing.xsl"), LISTING).expect("write the stylesheet");
        Xsltproc { dir }
    }

    /// The listing `xsltproc` gives, or `None` when libxml2 will not read the
    /// document.
    fn listing(&self, bytes: &[u8]) -> Option<String> {
        let source = self.dir.path().join("document.xml");
        std::fs::write(&source, bytes).expect("write the document");
        let out = Command::new("xsltproc")
            .arg("--nonet")
            .arg("--nowrite")
            .arg(self.dir.path().join("listing.xsl"))
            .arg(&source)
            .output()
            .expect("run xsltproc");
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).expect("xsltproc wrote UTF-8"))
    }
}

/// Where two listings first part, for the failure message.
fn first_difference(model: &[String], reference: &[String]) -> String {
    let at = model
        .iter()
        .zip(reference)
        .position(|(a, b)| a != b)
        .unwrap_or(model.len().min(reference.len()));
    format!(
        "line {}: model {:?}, xsltproc {:?} (lengths {} and {})",
        at + 1,
        model.get(at),
        reference.get(at),
        model.len(),
        reference.len()
    )
}

// ---------------------------------------------------------------------------
// Complete, and in order: a sample
// ---------------------------------------------------------------------------

/// Documents chosen for what they make hard, not at random.
///
/// Each line says what it exercises. The counts beside them are TLHdig Beta
/// 0.3, measured 2026-09-13.
const HARD: [&str; 29] = [
    // the three documents with comments
    "CTH 627_XML_HFR/IBoT 4.193.xml",
    "CTH 664_XML_HFR/KBo 67.110.xml",
    "CTH 670_XML_HFR/CTH 670-3426-3450/Privat 148.xml",
    // most elements in one document (9 171), and among the deepest nesting of
    // the documents the model reads. The deepest of all, `CTH 420_XML_TLH/KBo
    // 59.74.xml` at 80, is one of the 206 and has no model to compare.
    "CTH 561_XML_HDivT/KUB 5.1+.xml",
    "CTH 592_XML_HFR/IBoT 4.62+.xml",
    // two identical stylesheet instructions, and mixed content throughout
    "CTH 475_XML_HFR/KBo 43.119.xml",
    // an XML declaration (442 documents)
    "CTH 78_XML_SVH/CHDS 7.68.xml",
    // carriage returns (6 111 documents)
    "CTH 786_XML_HFR/KBo 17.86+.xml",
    // elements in the OpenDocument `text` namespace (73 documents)
    "CTH 635_XML_HFR/KBo 25.86+.xml",
    // a tab or line end inside an attribute value — all eleven
    "CTH 647_XML_HFR/KBo 64.45+.xml",
    "CTH 647_XML_HFR/CHDS 7.14+.xml",
    "CTH 678_XML_HFR/KUB 58.31+.xml",
    "CTH 744_XML_HFR/KBo 58.128+.xml",
    "CTH 694_XML_HFR/KBo 14.88+.xml",
    "CTH 701_XML_HFR/CHDS 7.219+.xml",
    "CTH 610_XML_HFR/KBo 51.219+.xml",
    "CTH 638_XML_HFR/KBo 54.125+.xml",
    "CTH 616_XML_HFR/CHDS 7.169.xml",
    "CTH 670_XML_HFR/CTH 670-1976-2000/CHDS 7.62.xml",
    "CTH 500_XML_HFR/CTH 500-351-400/CHDS 7.54+.xml",
    // `&amp;` or a numeric character reference — all seven
    "CTH 635_XML_HFR/KUB 46.13+.xml",
    "CTH 321_XML_MYTH/KBo 3.7.xml",
    "CTH 428_XML_TLH/CHDS 4.125.xml",
    "CTH 243_XML_PTAC/KBo 9.90+.xml",
    "CTH 244_XML_PTAC/KBo 18.162+.xml",
    "CTH 344_XML_MYTH/KBo 52.10+.xml",
    "CTH 573_XML_HDivT/KUB 16.64.xml",
    // an `AO:InvNr` and a `cth` element, two of the contract's metadata fields
    "CTH 17_XML_HAnn/KUB 23.117.xml",
    "CTH 598_XML_HFR/HT 51.xml",
];

/// Besides the hard ones, every thousandth admitted document, so the sample is
/// not only the corners.
const EVERY: usize = 1_000;

#[test]
fn the_model_says_what_xsltproc_says_for_the_sample() {
    let Some(path) = required() else { return };
    if !xsltproc_present() {
        return;
    }
    let reference = Xsltproc::new();
    let (mut compared, mut lines, mut hard_seen) = (0usize, 0usize, 0usize);
    let mut differing: Vec<String> = Vec::new();
    let mut order_checked = false;
    let mut index = 0usize;

    each_document(&path, |name, bytes| {
        index += 1;
        let hard = HARD.contains(&name);
        if !hard && !index.is_multiple_of(EVERY) {
            return;
        }
        hard_seen += usize::from(hard);
        let Ok(document) = Document::read(bytes) else {
            assert!(!hard, "{name} was chosen as readable and was refused");
            return;
        };
        let model = comparable(&listing(&document));
        let expected = comparable(
            &reference
                .listing(bytes)
                .unwrap_or_else(|| panic!("{name}: the model read it and libxml2 did not")),
        );
        if model != expected {
            differing.push(format!("{name}: {}", first_difference(&model, &expected)));
        }
        compared += 1;
        lines += model.len();

        // The comparison sees order, not only membership: the same lines with
        // two neighbouring attributes exchanged must fail it. Checked once, on
        // the document with the most elements.
        if name == "CTH 561_XML_HDivT/KUB 5.1+.xml" {
            let pair = model
                .windows(2)
                .position(|w| w[0].starts_with("A ") && w[1].starts_with("A ") && w[0] != w[1])
                .expect("two neighbouring attributes");
            let mut swapped = model.clone();
            swapped.swap(pair, pair + 1);
            assert_ne!(swapped, expected, "a reordering went unnoticed");
            order_checked = true;
        }
    });

    assert!(
        differing.is_empty(),
        "{} of {compared} documents differ from xsltproc: {differing:#?}",
        differing.len()
    );
    assert!(order_checked, "the order control did not run");
    eprintln!(
        "sample: {compared} documents, {lines} listing lines identical to xsltproc, \
         {hard_seen} of them chosen"
    );
}

/// The same comparison over every document the model reads.
#[test]
#[ignore = "one xsltproc run per document: minutes; run by hand"]
fn the_model_says_what_xsltproc_says_for_the_whole_corpus() {
    let Some(path) = required() else { return };
    if !xsltproc_present() {
        return;
    }
    let reference = Xsltproc::new();
    let (mut same, mut lines, mut refused) = (0usize, 0usize, 0usize);
    let mut differing: Vec<String> = Vec::new();
    each_document(&path, |name, bytes| {
        let Ok(document) = Document::read(bytes) else {
            refused += 1;
            return;
        };
        let model = comparable(&listing(&document));
        let expected = reference
            .listing(bytes)
            .map(|listing| comparable(&listing))
            .unwrap_or_default();
        if model == expected {
            same += 1;
            lines += model.len();
        } else {
            differing.push(format!("{name}: {}", first_difference(&model, &expected)));
        }
    });
    assert!(
        differing.is_empty(),
        "{} documents differ from xsltproc; first: {:#?}",
        differing.len(),
        &differing[..differing.len().min(10)]
    );
    eprintln!("whole corpus: {same} identical ({lines} lines), {refused} refused by the model");
}

// ---------------------------------------------------------------------------
// Stable: the whole corpus, twice
// ---------------------------------------------------------------------------

/// What reading one document came to: the digest of its listing, or the key of
/// the refusal.
fn outcome(bytes: &[u8]) -> String {
    match Document::read(bytes) {
        Ok(document) => md5_hex(listing(&document).as_bytes()),
        Err(refusal) => format!("refused: {}", key(&refusal)),
    }
}

fn key(refusal: &Refusal) -> String {
    match refusal {
        Refusal::Unread(finding) => format!("unread/{}", finding.reason.key()),
        Refusal::BeyondTheParser(beyond) => format!("beyond/{}", beyond.limit.key()),
        Refusal::NotUtf8 { .. } => "not-utf8".into(),
        Refusal::Unexplained { .. } => "unexplained".into(),
        Refusal::UndeclaredPrefix { .. } => "undeclared-prefix".into(),
        Refusal::Undecided { what, .. } => format!("undecided/{}", what.key()),
        Refusal::OutsideRoot { .. } => "outside-root".into(),
        Refusal::NoRoot => "no-root".into(),
    }
}

const DOCUMENTS: usize = 23_936;

#[test]
fn the_whole_corpus_reads_the_same_twice_and_refuses_what_the_manifest_names() {
    let Some(path) = required() else { return };

    let mut first: Vec<(String, String)> = Vec::with_capacity(DOCUMENTS);
    let mut disagreements: Vec<String> = Vec::new();
    let (mut declarations, mut stylesheets, mut stylesheet_documents) = (0usize, 0usize, 0usize);
    let (mut declarations_in_bytes, mut stylesheets_in_bytes) = (0usize, 0usize);
    let mut comment_documents = 0usize;
    let mut deepest: (usize, String) = (0, String::new());

    let admitted = each_document(&path, |name, bytes| {
        let result = Document::read(bytes);
        // The refusals are the manifest's: the classifier's 206, then the
        // scanner's seventeen among what the classifier accepts.
        let named = classify(bytes).is_some() || beyond_the_parser(bytes).is_some();
        if named != result.is_err() {
            disagreements.push(format!(
                "{name}: model {:?}, manifest names it: {named}",
                result.as_ref().err()
            ));
        }
        if let Ok(document) = &result {
            // Nesting depth of elements, counted on the model: a node is one
            // deeper than its parent, and parents come first in the vector.
            let mut depth = vec![0usize; document.nodes().len()];
            for (at, node) in document.nodes().iter().enumerate() {
                depth[at] = node.parent.map_or(1, |parent| depth[parent] + 1);
            }
            let here = depth.iter().copied().max().unwrap_or(0);
            if here > deepest.0 {
                deepest = (here, name.to_string());
            }
            // The same two counts taken from the bytes, over the same
            // documents: the model neither drops nor invents a declaration or
            // an instruction.
            declarations_in_bytes += usize::from(bytes.starts_with(b"<?xml "));
            stylesheets_in_bytes += bytes
                .windows(b"<?xml-stylesheet".len())
                .filter(|window| window == b"<?xml-stylesheet")
                .count();
            declarations += usize::from(document.declaration().is_some());
            let here = document
                .nodes()
                .iter()
                .filter(|node| matches!(&node.kind, Kind::Instruction { target, .. } if target == "xml-stylesheet"))
                .count();
            stylesheets += here;
            stylesheet_documents += usize::from(here > 0);
            comment_documents += usize::from(
                document
                    .nodes()
                    .iter()
                    .any(|node| matches!(node.kind, Kind::Comment(_))),
            );
        }
        first.push((name.to_string(), outcome(bytes)));
    });

    let mut second: Vec<(String, String)> = Vec::with_capacity(DOCUMENTS);
    each_document(&path, |name, bytes| {
        second.push((name.to_string(), outcome(bytes)))
    });

    assert_eq!(admitted, DOCUMENTS);
    assert!(
        disagreements.is_empty(),
        "the model and the manifest disagree on {} documents: {disagreements:#?}",
        disagreements.len()
    );
    let moved: Vec<&String> = first
        .iter()
        .zip(&second)
        .filter(|(a, b)| a != b)
        .map(|(a, _)| &a.0)
        .collect();
    assert!(
        moved.is_empty(),
        "read differently the second time: {moved:#?}"
    );

    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, result) in &first {
        let bucket = result.strip_prefix("refused: ").unwrap_or("built");
        *tally.entry(bucket).or_default() += 1;
    }
    eprintln!("model over the corpus, twice: {tally:#?}");
    eprintln!(
        "declarations {declarations}, stylesheet instructions {stylesheets} in \
         {stylesheet_documents} documents, documents with comments {comment_documents}"
    );

    let refused_unread: usize = tally
        .iter()
        .filter(|(bucket, _)| bucket.starts_with("unread/"))
        .map(|(_, n)| n)
        .sum();
    let refused_beyond: usize = tally
        .iter()
        .filter(|(bucket, _)| bucket.starts_with("beyond/"))
        .map(|(_, n)| n)
        .sum();
    assert_eq!(tally.get("built"), Some(&23_713));
    assert_eq!(refused_unread, 206);
    assert_eq!(refused_beyond, 17);
    assert_eq!(tally.get("unexplained"), None);

    // Over the documents the model reads, not the whole corpus: the contract's
    // 442 declarations and 8 424 stylesheet instructions include the ones in
    // the 223 refused documents, which have no model. So the model is held to
    // the bytes of the same documents, and the difference is not a loss.
    assert_eq!(declarations, declarations_in_bytes);
    assert_eq!(stylesheets, stylesheets_in_bytes);
    assert_eq!(comment_documents, 3);
    eprintln!(
        "deepest nesting among the documents read: {} ({})",
        deepest.0, deepest.1
    );
}
