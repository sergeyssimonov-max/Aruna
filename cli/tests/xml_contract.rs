//! The contract between the corpus and this program, checked against fixtures.
//!
//! Three things are being held to account here, in the order the project ranks
//! them:
//!
//! 1. The source is never written to. Every fixture is read, put through
//!    everything this program does, and compared byte for byte afterwards.
//! 2. Nothing is lost or invented between a document and its normalised form,
//!    beyond a permit list that is stated in one place and checked here.
//! 3. Nothing panics, whatever the document is — including the four classes of
//!    malformed XML the real corpus actually contains.
//!
//! Every fixture is synthetic and is described in `fixtures/xml/MANIFEST.md`.
//! Where one reproduces something the corpus really has, the manifest says how
//! many documents that is.

use aruna::export::{normalize_into, verify};
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, parse_manuscript, HEADER_READ_LIMIT};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/xml")
}

/// Every fixture, as (relative name, bytes).
fn all() -> Vec<(String, Vec<u8>)> {
    let root = fixtures();
    let mut out = Vec::new();
    for group in ["valid", "malformed", "hostile"] {
        let dir = root.join(group);
        let mut names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".xml"))
            .collect();
        names.sort();
        for name in names {
            let path = dir.join(&name);
            let bytes = std::fs::read(&path).expect("read fixture");
            out.push((format!("{group}/{name}"), bytes));
        }
    }
    out
}

/// A plausible archive path for a fixture, so the corpus's own gates accept it.
fn archive_path(name: &str) -> String {
    format!("root/CTH 5_XML_HFR/{}", name.replace('/', "-"))
}

#[test]
fn the_manifest_and_the_directory_describe_the_same_fixtures() {
    let manifest = std::fs::read_to_string(fixtures().join("MANIFEST.md")).expect("manifest");
    let described: BTreeSet<String> = manifest
        .lines()
        .filter_map(|line| line.strip_prefix("### `")?.strip_suffix("`"))
        .map(str::to_string)
        .collect();
    let present: BTreeSet<String> = all().into_iter().map(|(name, _)| name).collect();

    let missing: Vec<_> = present.difference(&described).collect();
    let stale: Vec<_> = described.difference(&present).collect();
    assert!(
        missing.is_empty(),
        "fixtures with no manifest entry: {missing:?}"
    );
    assert!(
        stale.is_empty(),
        "manifest entries with no fixture: {stale:?}"
    );

    // The recorded size catches an accidental edit without this test carrying a
    // second digest implementation; `SHA256SUMS` is the real check and the
    // manifest says how to run it.
    for (name, bytes) in all() {
        let head = manifest
            .split(&format!("### `{name}`"))
            .nth(1)
            .expect("entry");
        let recorded: usize = head
            .lines()
            .find_map(|l| l.trim().strip_prefix("- bytes: "))
            .expect("a byte count")
            .parse()
            .expect("a number");
        assert_eq!(recorded, bytes.len(), "{name} changed size");
    }
}

#[test]
fn no_fixture_is_written_to_by_anything_this_program_does() {
    let before = all();
    for (name, bytes) in &before {
        // Everything the program does to a document, in order.
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let path = archive_path(name);
        let _ = is_manuscript_xml(&path);
        let _ = looks_like_manuscript(&head);
        let _ = parse_manuscript(&path, &head);
        let mut out = Vec::new();
        normalize_into(bytes, &mut out);
        let _ = verify::compare(bytes, &out);
    }
    let after = all();
    assert_eq!(
        before.len(),
        after.len(),
        "the fixture directory gained or lost a file"
    );
    for ((name, before), (_, after)) in before.iter().zip(&after) {
        assert_eq!(before, after, "{name} was modified");
    }
}

#[test]
fn well_formed_fixtures_normalise_without_distortion() {
    for (name, bytes) in all() {
        if !name.starts_with("valid/") || name == "valid/declared-latin1.xml" {
            continue;
        }
        let mut out = Vec::new();
        normalize_into(&bytes, &mut out);
        let report = verify::compare(&bytes, &out)
            .unwrap_or_else(|why| panic!("{name} was distorted: {why}"));

        // Whatever was dropped is on the permit list, which is what `compare`
        // returning `Ok` already means; this checks the report says so too.
        for rule in &report.dropped {
            assert!(
                verify::is_dropped(rule.as_bytes()),
                "{name} reports dropping <?{rule}…?>, which is not on the permit list"
            );
        }
        assert!(
            out.starts_with(verify::DECLARATION),
            "{name} lost its canonical declaration"
        );
    }
}

/// The one permitted change that is not permitted after all.
///
/// Replacing the declaration is on the list. Replacing what it says about the
/// encoding is not: the body is copied byte for byte, so the bytes would stay
/// and their meaning would change — and the byte comparison would call that no
/// distortion, because by that measure it is none.
#[test]
fn a_declaration_that_would_change_the_encoding_is_refused() {
    let bytes = std::fs::read(fixtures().join("valid/declared-latin1.xml")).expect("fixture");
    assert!(
        bytes.contains(&0xE9),
        "the fixture is supposed to carry a byte that is not UTF-8"
    );
    let mut out = Vec::new();
    normalize_into(&bytes, &mut out);

    assert!(
        std::str::from_utf8(&out).is_err(),
        "the fixture is supposed to become invalid UTF-8 once relabelled"
    );
    let why = verify::compare(&bytes, &out).expect_err("this must not be allowed through");
    assert!(why.contains("ISO-8859-1"), "{why}");
    assert!(why.contains("UTF-8"), "{why}");
}

#[test]
fn an_undeclared_or_utf8_document_is_allowed_through() {
    for source in [
        &b"<AOxml><a/></AOxml>"[..],
        b"<?xml version=\"1.0\"?><AOxml><a/></AOxml>",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><AOxml><a/></AOxml>",
        b"<?xml version=\"1.0\" encoding=\"utf8\"?><AOxml><a/></AOxml>",
    ] {
        let mut out = Vec::new();
        normalize_into(source, &mut out);
        verify::compare(source, &out).unwrap_or_else(|why| {
            panic!(
                "{} was refused: {why}",
                String::from_utf8_lossy(&source[..source.len().min(50)])
            )
        });
    }
}

#[test]
fn documents_that_are_not_well_formed_are_read_without_panic_and_kept_verbatim() {
    for (name, bytes) in all() {
        if !name.starts_with("malformed/") {
            continue;
        }
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let record = parse_manuscript(&archive_path(&name), &head);
        // The parser never refuses; it reports what it could not find.
        assert!(!record.sigla.is_empty(), "{name} produced no siglum at all");

        let mut out = Vec::new();
        normalize_into(&bytes, &mut out);
        // The body is not this program's to repair. Everything after the
        // prologue must survive exactly, malformed or not.
        let tail = |v: &[u8]| {
            let at = v
                .windows(9)
                .position(|w| w == b"<AOHeader")
                .unwrap_or_default();
            v[at..].to_vec()
        };
        if !bytes.is_empty() {
            assert_eq!(tail(&bytes), tail(&out), "{name} was repaired or damaged");
        }
    }
}

#[test]
fn the_fields_the_manifest_promises_are_the_fields_extracted() {
    // A path with no CTH in it, so the header is what answers.
    let neutral = |name: &str| format!("root/unfiled_XML_HFR/{}", name.replace('/', "-"));
    let cases: [(&str, &str, Option<&str>); 5] = [
        ("valid/minimal.xml", "KBo 1.1", None),
        ("valid/typical.xml", "KUB 2.1", Some("CTH 561")),
        ("valid/sections.xml", "KBo 3.5", Some("CTH 1")),
        ("valid/duplicate-ids.xml", "KBo 5.1", None),
        ("valid/deep.xml", "KBo 59.74", None),
    ];
    for (name, siglum, cth) in cases {
        let bytes = std::fs::read(fixtures().join(name)).expect("fixture");
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let record = parse_manuscript(&neutral(name), &head);
        assert_eq!(record.sigla, siglum, "{name}");
        assert_eq!(record.cth.as_deref(), cth, "{name}");
    }
}

/// Which source of truth wins, when two of them disagree.
///
/// The archive folder does. It is how the corpus is filed, and it is what the
/// package's own folders are named after — a document that says one group while
/// sitting in another would otherwise be linked from a group it is not in.
/// Pinned here because the next stage reads this grouping to build bookmarks,
/// and a converter that resolves the tie the other way would produce a table of
/// contents that disagrees with the folders beside it.
#[test]
fn the_folder_decides_the_group_when_it_and_the_header_disagree() {
    let bytes = std::fs::read(fixtures().join("valid/typical.xml")).expect("fixture");
    let head = String::from_utf8_lossy(&bytes);
    assert!(
        head.contains("<CTHNr>CTH 561</CTHNr>"),
        "the fixture is supposed to name a group in its header"
    );
    let filed_elsewhere = parse_manuscript("root/CTH 5_XML_HFR/KUB 2.1.xml", &head);
    assert_eq!(filed_elsewhere.cth.as_deref(), Some("CTH 5"));
    let unfiled = parse_manuscript("root/unfiled_XML_HFR/KUB 2.1.xml", &head);
    assert_eq!(unfiled.cth.as_deref(), Some("CTH 561"));
}

#[test]
fn the_gates_refuse_an_empty_document_and_accept_the_rest() {
    for (name, bytes) in all() {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
        let accepted = looks_like_manuscript(&head);
        // Only a document with nothing recognisable in its first 16 KiB is
        // refused. `not-utf8.xml` opens with bytes that are not UTF-8 and is
        // still accepted: the header is decoded lossily, which leaves the
        // markup after the junk readable, and refusing a document because its
        // first two bytes are wrong would lose a manuscript over a prefix.
        if name == "malformed/empty.xml" {
            assert!(!accepted, "{name} should not pass the content gate");
        } else {
            assert!(accepted, "{name} should pass the content gate");
        }
    }
}
