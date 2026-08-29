//! What reaches the package is the document that was in the archive.
//!
//! The project ranks this promise first, and three things already guard parts
//! of it: `export::verify` refuses a document that changed beyond the permit
//! list, `tests/corpus.rs` runs that refusal over all 23 936 documents, and
//! `tests/reliability.rs` shows two builds of one archive are byte-identical.
//!
//! **None of them looks at the files on disk.** `corpus.rs` compares a source
//! document with `normalize_into` of the same bytes, in memory; the export then
//! writes its own copy, and nothing asserted that what it wrote is what was
//! checked. Between the check and the disk lie the second pass over the
//! archive, the placement, the staging directory and the publish rename — and
//! a document lost, duplicated or truncated in any of them would leave every
//! existing test green.
//!
//! So this file asks the question from the other end: take the archive and take
//! the published package, and match them **as multisets of file contents**. If
//! each side holds exactly the same documents with the same multiplicity, then
//! nothing was lost, nothing was invented, nothing was altered, and nothing was
//! written twice — all four, from one comparison, without needing to know which
//! file came from which entry.
//!
//! **The permit list is re-implemented here rather than borrowed.** The
//! expected output of one document is built below from the rule as the
//! specification states it — canonical declaration, `xml` and `xml-stylesheet`
//! instructions dropped, any other instruction kept and followed by one
//! newline, prologue whitespace not carried over, everything from the first
//! byte that is not prologue copied unchanged. A test that called
//! `normalize_into` would agree with the normaliser about its own bugs.
//!
//! Run the heavy half against the real corpus:
//!
//! ```sh
//! cargo nextest run --test authenticity --run-ignored ignored-only
//! ```

use aruna::export::verify::{self, DECLARATION};
use aruna::export::{self, PACKAGE};
use aruna::md5::md5_hex;
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, HEADER_READ_LIMIT};
use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

/// What TLHdig Beta 0.3 holds, and what the specification records in 3.6.
const DOCUMENTS: usize = 23_936;
/// The documents, plus the inventory and the manifest.
const FILES_IN_PACKAGE: usize = DOCUMENTS + 2;

/// The document as the package is expected to hold it, built from the permit
/// list rather than from the normaliser.
///
/// Four rules, and no fifth: a leading byte-order mark goes, the canonical
/// declaration is written once at the front, an `xml` or `xml-stylesheet`
/// instruction in the prologue is dropped, any other instruction is kept and
/// followed by exactly one newline. Whitespace inside the prologue is not
/// carried over — it is the one place this transform reflows anything — and
/// from the first byte that is not prologue, the document is copied.
fn expected_output(source: &[u8]) -> Vec<u8> {
    let mut body = source;
    if let Some(rest) = body.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        body = rest;
    }

    let mut out = Vec::with_capacity(source.len() + DECLARATION.len());
    out.extend_from_slice(DECLARATION);

    let mut i = 0usize;
    loop {
        while matches!(body.get(i), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            i += 1;
        }
        if !body[i..].starts_with(b"<?") {
            break;
        }
        let Some(close) = find(&body[i..], b"?>") else {
            break;
        };
        let pi = &body[i..i + close + 2];
        if !verify::is_dropped(target_of(pi)) {
            out.extend_from_slice(pi);
            out.push(b'\n');
        }
        i += close + 2;
    }

    out.extend_from_slice(&body[i..]);
    out
}

/// The name after `<?`, up to the first space or `?`.
fn target_of(pi: &[u8]) -> &[u8] {
    let rest = pi.get(2..).unwrap_or_default();
    let end = rest
        .iter()
        .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'?'))
        .unwrap_or(rest.len());
    &rest[..end]
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// One digest per document, with how many documents hash to it.
///
/// A multiset rather than a set: the corpus files 28 documents under two
/// catalogue numbers each, so identical contents appearing twice is a fact
/// about the source and has to survive into the comparison rather than be
/// folded away by it.
type Bodies = BTreeMap<String, usize>;

fn count(bodies: &mut Bodies, digest: String) {
    *bodies.entry(digest).or_default() += 1;
}

/// Every document the gates admit, as the package is expected to hold it.
fn expected_from_archive(archive: &Path) -> (Bodies, usize) {
    let file = std::fs::File::open(archive).expect("open the archive");
    let mut zip =
        zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file)).expect("read");

    let mut bodies = Bodies::new();
    let mut admitted = 0usize;
    let mut source = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read the entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        admitted += 1;
        count(&mut bodies, md5_hex(&expected_output(&source)));
    }

    (bodies, admitted)
}

/// Every `.xml` file the package holds, and every file in it.
fn published(root: &Path) -> (Bodies, usize) {
    let mut bodies = Bodies::new();
    let mut files = 0usize;
    walk(root, &mut |path| {
        files += 1;
        if path.extension().and_then(|e| e.to_str()) == Some("xml") {
            let bytes = std::fs::read(path).expect("read a published document");
            assert!(
                bytes.starts_with(DECLARATION),
                "{} does not begin with the canonical declaration",
                path.display()
            );
            count(&mut bodies, md5_hex(&bytes));
        }
    });
    (bodies, files)
}

fn walk(dir: &Path, each: &mut impl FnMut(&Path)) {
    for entry in std::fs::read_dir(dir).expect("read the package").flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, each);
        } else {
            each(&path);
        }
    }
}

/// The two sides, matched, with the difference spelled out when they are not.
fn assert_same_documents(archive: &Path, root: &Path, admitted: usize, in_package: usize) {
    let (expected, _) = expected_from_archive(archive);
    let (found, files) = published(root);

    let mut missing: Vec<(&String, usize, usize)> = Vec::new();
    for (digest, wanted) in &expected {
        let got = found.get(digest).copied().unwrap_or(0);
        if got != *wanted {
            missing.push((digest, *wanted, got));
        }
    }
    let invented: Vec<&String> = found
        .keys()
        .filter(|d| !expected.contains_key(*d))
        .collect();

    assert!(
        missing.is_empty(),
        "{} document(s) reached the package in the wrong number; first few (digest, in archive, in package): {:?}",
        missing.len(),
        &missing[..missing.len().min(5)]
    );
    assert!(
        invented.is_empty(),
        "{} document(s) in the package came from no archive entry: {:?}",
        invented.len(),
        &invented[..invented.len().min(5)]
    );
    assert_eq!(
        admitted, in_package,
        "the number of documents written is not the number the gates admitted"
    );
    assert_eq!(
        files,
        in_package + 2,
        "the package holds files that are neither a document, the inventory nor the manifest"
    );
}

/// An archive small enough to read, carrying the things a corpus document
/// actually carries: a Windows line ending, cuneiform above the basic plane, a
/// decomposed diacritic, preserved whitespace, an instruction the package does
/// not drop, a comment, an entity, a namespace prefix — and one document filed
/// under two catalogue numbers, which the real corpus does 28 times.
fn write_archive(path: &Path) {
    use std::io::Write as _;
    let file = std::fs::File::create(path).expect("create the archive");
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut add = |name: &str, body: &str| {
        zip.start_file(name, options).expect("start");
        zip.write_all(body.as_bytes()).expect("write");
    };

    // `a\u{301}` is a decomposed acute: NFC would compose it, and nothing here
    // may. `\u{12000}` is cuneiform, four bytes and outside the basic plane.
    let plain = "<?xml-stylesheet href=\"HPMxml.css\" type=\"text/css\"?>\
         <AOxml xmlns:AO=\"http://hethiter.net/ns/AO/1.0\" xml:space=\"preserve\">\r\n\
         <AOHeader><docID>KBo 1.1</docID>\
         <meta><uebern editor=\"FB\" date=\"2017-03-28\"/></meta></AOHeader>\
         <!-- an editorial note --><body><text><l lg=\"Hit\"/>  a\u{301}  \u{12000}  &amp;  \
         <AO:InvNr>VAT 1</AO:InvNr></text></body></AOxml>";
    let kept_pi = format!("<?some-tool note=\"keep me\"?>{plain}");
    let declared = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n{plain}");

    add("root/CTH 5_XML_HFR/KBo 1.1.xml", plain);
    add("root/CTH 5_XML_HFR/KUB 2.1.xml", &kept_pi);
    add("root/CTH 9_XML_TLH/KBo 3.22.xml", &declared);
    // The same bytes under a second catalogue number: both copies must survive.
    add("root/CTH 9_XML_HFR/KBo 1.1.xml", plain);

    // Debris the gates keep out; none of it may reach the package.
    add("__MACOSX/root/CTH 5_XML_HFR/._KBo 1.1.xml", "resource fork");
    add("root/CTH 5_XML_HFR/HPMxml.css", "body { }");

    zip.finish().expect("finish");
}

fn build(dir: &Path, archive: &Path) -> (export::Built, PathBuf) {
    let built = export::build(archive, dir, "test source", &aruna::job::Job::unattended())
        .expect("the package builds");
    (built, dir.join(PACKAGE))
}

/// **Every admitted document is in the package once, and unchanged.**
///
/// The properties this settles in one comparison, each of which the corpus
/// really exercises: a `\r\n` survives as `\r\n`; a decomposed diacritic is not
/// composed; a character above the basic plane is not split or replaced; the
/// whitespace inside `xml:space="preserve"` is untouched; an entity is neither
/// resolved nor re-escaped; a comment, a namespace prefix and the order of
/// attributes are all left as written; an instruction the permit list does not
/// name is kept rather than dropped; and two entries whose contents are equal
/// both arrive rather than one overwriting the other.
#[test]
fn a_package_holds_every_admitted_document_once_and_unchanged() {
    let dir = tempdir().expect("tempdir");
    let archive = dir.path().join("corpus.zip");
    write_archive(&archive);
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    let (built, root) = build(&destination, &archive);

    assert_eq!(built.documents, 4, "the debris is not a document");
    assert_same_documents(&archive, &root, 4, built.documents);
}

/// The same question, asked of the corpus this program exists for.
///
/// Heavy on purpose — it builds the whole 384 MB package into a temporary
/// directory and reads every file back — so it is `#[ignore]` and the ordinary
/// `nextest` run stays under ten seconds. It is the acceptance check for the
/// authenticity contour, and the anchors it asserts are the ones the
/// specification records in 3.6: 23 936 documents, 23 938 files.
///
/// Skipped, like `tests/corpus.rs`, when the archive is not on this machine;
/// `ARUNA_REQUIRE_FIXTURE=1` turns that skip into a failure.
#[test]
#[ignore = "builds the whole corpus; run with --run-ignored ignored-only"]
fn the_whole_corpus_reaches_the_package_once_and_unchanged() {
    let Some(archive) = fixture() else { return };

    let dir = tempdir().expect("tempdir");
    let (built, root) = build(dir.path(), &archive);

    assert_eq!(
        built.documents, DOCUMENTS,
        "the export wrote a different number of documents than the corpus holds"
    );
    assert_same_documents(&archive, &root, DOCUMENTS, built.documents);

    let (_, files) = published(&root);
    assert_eq!(
        files, FILES_IN_PACKAGE,
        "the package holds a different number of files than 3.6 records"
    );
}

/// The archive, wherever this run keeps it — the same three names
/// `tests/corpus.rs` looks under, so one machine's layout serves both.
fn fixture() -> Option<PathBuf> {
    for name in ["ARUNA_ZIP", "ARUNA_FIXTURE_ZIP"] {
        if let Some(named) = std::env::var_os(name) {
            let path = PathBuf::from(named);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
    if default.is_file() {
        return Some(default);
    }
    assert!(
        std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
        "ARUNA_REQUIRE_FIXTURE is set but the corpus archive is missing"
    );
    eprintln!("skipping: the corpus archive is not present");
    None
}
