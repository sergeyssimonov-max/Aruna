//! The whole corpus, against the guarantees this project makes about it.
//!
//! Skipped when the archive is not there — it is 71 MiB and is not committed.
//! Set `ARUNA_REQUIRE_FIXTURE=1` to turn a missing archive into a failure, which
//! is what CI does after downloading it.
//!
//! ```sh
//! # the archive lives at cli/fixtures/, or name another with ARUNA_ZIP
//! cargo nextest run --test corpus
//! ARUNA_REQUIRE_FIXTURE=1 cargo nextest run --test corpus
//! ```
//!
//! What is checked here is the promise the project ranks first: that reading
//! the corpus does not change it, and that normalising a document changes
//! nothing the permit list does not name. Both were previously demonstrated by
//! a program someone had to remember to run.
//!
//! The counts asserted below are anchors, not specifications. They come from
//! `cargo run --release --example corpus_inventory` against TLHdig Beta 0.3 and
//! their job is to notice when a change to the gates silently admits or drops
//! documents. A new edition of the corpus is expected to move them, and moving
//! them is a deliberate edit here.

use aruna::export::{normalize_into, verify};
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, HEADER_READ_LIMIT};
use std::io::Read as _;
use std::path::PathBuf;

/// What TLHdig Beta 0.3 holds.
const DOCUMENTS: usize = 23_936;
/// Documents whose start and end tags do not match. One of four classes of
/// malformed XML in this corpus; see the test at the bottom of this file.
///
/// Larger than the number of documents `xmllint` blames on a tag mismatch,
/// because `xmllint` stops at the first error and most of these have an
/// attribute error before it. Every one of them is inside the 210 it rejects.
const TAGS_DO_NOT_BALANCE: usize = 121;

fn archive() -> Option<PathBuf> {
    if let Some(named) = std::env::var_os("ARUNA_ZIP") {
        return Some(PathBuf::from(named));
    }
    let default = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
    default.is_file().then_some(default)
}

/// The archive, or a note saying why this test did nothing.
fn required() -> Option<PathBuf> {
    match archive() {
        Some(path) if path.is_file() => Some(path),
        other => {
            assert!(
                std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
                "ARUNA_REQUIRE_FIXTURE is set but {:?} is missing",
                other
            );
            eprintln!("skipping: the corpus archive is not present");
            None
        }
    }
}

#[test]
fn the_whole_corpus_normalises_without_distortion_and_the_archive_is_not_written_to() {
    let Some(path) = required() else { return };

    let before = aruna::md5::md5_file(&path).expect("digest");
    let file = std::fs::File::open(&path).expect("open");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let (mut checked, mut refused) = (0usize, 0usize);
    let mut distorted: Vec<String> = Vec::new();
    let mut applied: std::collections::BTreeMap<String, usize> = Default::default();
    let mut source = Vec::new();
    let mut out = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            refused += 1;
            continue;
        }
        checked += 1;

        out.clear();
        normalize_into(&source, &mut out);
        match verify::compare(&source, &out) {
            Ok(report) => {
                for rule in report.dropped {
                    assert!(
                        verify::is_dropped(rule.as_bytes()),
                        "{name} dropped <?{rule}…?>, which is not on the permit list"
                    );
                    *applied.entry(format!("DROP_PI {rule}")).or_default() += 1;
                }
                if report.added_declaration {
                    *applied.entry("ADD declaration".into()).or_default() += 1;
                }
                if report.reflowed {
                    *applied.entry("REFLOW".into()).or_default() += 1;
                }
            }
            Err(why) if distorted.len() < 10 => distorted.push(format!("{name}: {why}")),
            Err(_) => {}
        }
    }

    assert!(
        distorted.is_empty(),
        "{} document(s) distorted, first few: {distorted:#?}",
        distorted.len()
    );
    assert_eq!(
        checked, DOCUMENTS,
        "the gates admitted a different number of documents than the inventory recorded"
    );
    assert!(
        refused > 0,
        "the debris in the archive should still be refused"
    );
    eprintln!("corpus: {checked} documents, permitted changes applied: {applied:?}");

    let after = aruna::md5::md5_file(&path).expect("digest");
    assert_eq!(before, after, "the archive was written to");
}

/// The corpus contains documents no conforming XML parser will accept.
///
/// 210 of 23 936, measured with `xmllint --noout`. `xmllint` reports the first
/// error in each and blames: an attribute name that is not a name (82), a raw
/// `<` inside an attribute value (44), a tag mismatch (54), a qualified name
/// with an empty local part (13), and a handful of others. Counting the whole
/// document rather than its first error, 121 have tags that do not balance —
/// most of those also have an attribute error earlier, which is what `xmllint`
/// stops on. All four classes are reproduced in `fixtures/xml/malformed/`.
///
/// Only the tag-mismatch class is counted here, because it is the only one this
/// project can measure without shipping an XML parser it does not have. The
/// other three are recorded in `docs/XML-CONTRACT.md` with the command that
/// produced them.
///
/// None of this is a failure. The corpus is the source of truth and repairing it
/// in place is the one thing this project must never do. The number is for the
/// next stage: a converter built on a strict parser will refuse these documents
/// and needs a stated policy before it meets them rather than after.
#[test]
fn the_documents_whose_tags_do_not_balance_are_the_ones_already_known() {
    let Some(path) = required() else { return };
    let file = std::fs::File::open(&path).expect("open");
    let mut zip = zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file))
        .expect("read archive");

    let mut unbalanced = Vec::new();
    let mut source = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        entry.read_to_end(&mut source).expect("read entry");
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }
        if !balanced(&source) {
            unbalanced.push(name);
        }
    }

    assert_eq!(
        unbalanced.len(),
        TAGS_DO_NOT_BALANCE,
        "the corpus changed shape; first few: {:#?}",
        &unbalanced[..unbalanced.len().min(5)]
    );
}

/// Whether every end tag closes the element the last start tag opened.
///
/// Deliberately the smallest check that answers one question. It is not an XML
/// parser and must not grow into one: this project has no parser, and a partial
/// one living in a test is how a project ends up with two.
fn balanced(bytes: &[u8]) -> bool {
    let mut stack: Vec<&[u8]> = Vec::new();
    let mut i = 0usize;
    while let Some(open) = memchr_lt(&bytes[i..]) {
        i += open;
        let rest = &bytes[i..];
        let skip_to = |needle: &[u8]| {
            rest.windows(needle.len())
                .position(|w| w == needle)
                .map_or(rest.len(), |at| at + needle.len())
        };
        if rest.starts_with(b"<!--") {
            i += skip_to(b"-->");
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i += skip_to(b"]]>");
            continue;
        }
        if rest.starts_with(b"<?") || rest.starts_with(b"<!") {
            i += skip_to(b">");
            continue;
        }
        let end = skip_to(b">");
        let tag = &rest[..end];
        if rest.starts_with(b"</") {
            let name = local(&tag[2..tag.len().saturating_sub(1)]);
            match stack.pop() {
                Some(open) if open == name => {}
                _ => return false,
            }
        } else if !tag.ends_with(b"/>") {
            let inner = &tag[1..tag.len().saturating_sub(1)];
            let cut = inner
                .iter()
                .position(u8::is_ascii_whitespace)
                .unwrap_or(inner.len());
            stack.push(local(&inner[..cut]));
        }
        i += end;
    }
    stack.is_empty()
}

fn memchr_lt(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|b| *b == b'<')
}

/// The part of a qualified name after the colon.
fn local(qname: &[u8]) -> &[u8] {
    match qname.iter().position(|c| *c == b':') {
        Some(at) => &qname[at + 1..],
        None => qname,
    }
}
