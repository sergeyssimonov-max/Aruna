//! What a document written to attack the reader gets out of this program.
//!
//! The honest answer is "nothing", and the reason is structural rather than
//! defensive: there is no XML parser here. The dependency list has no XML crate
//! in it. What this program calls parsing is a scan for seven fields in the
//! first 16 KiB, and what it calls exporting is a byte copy. Nothing resolves an
//! entity, opens a DTD, follows an XInclude or expands anything, because there
//! is no code that could.
//!
//! That is worth more than a mitigation and it is also worth less: it holds only
//! for as long as it is true, and the next stage of this project needs a real
//! parser to turn these documents into PDF. These tests exist so that the day a
//! parser arrives, the properties it has to keep are already written down and
//! already failing if it does not.
//!
//! Every bound here is generous enough not to be flaky and tight enough to mean
//! something: a TCP connection to an unroutable address takes seconds to time
//! out, so completing in under two is evidence that none was attempted.

use aruna::export::{self, normalize_into, verify, PACKAGE};
use aruna::parse::{looks_like_manuscript, parse_manuscript, HEADER_READ_LIMIT};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/xml")
}

fn hostile(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join("hostile").join(name))
        .unwrap_or_else(|e| panic!("read {name}: {e}"))
}

/// Everything this program does to one document, and how long it took.
fn run(path: &str, bytes: &[u8]) -> (Duration, Vec<u8>) {
    let start = Instant::now();
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEADER_READ_LIMIT)]);
    let _ = looks_like_manuscript(&head);
    let record = parse_manuscript(path, &head);
    let mut out = Vec::new();
    normalize_into(bytes, &mut out);
    let _ = verify::compare(bytes, &out);
    // Keep the record alive so nothing above is optimised away.
    assert!(!record.sigla.is_empty());
    (start.elapsed(), out)
}

/// The bound every test here uses: long enough that a slow machine will not
/// trip it, short enough that a network round trip could not have happened.
const BUDGET: Duration = Duration::from_secs(2);

#[test]
fn an_entity_pointing_at_a_local_file_is_never_resolved() {
    let dir = tempdir().expect("tempdir");
    let canary = dir.path().join("canary.txt");
    let secret = "ARUNA-CANARY-8f3c1d2e-THIS-MUST-NOT-APPEAR";
    std::fs::write(&canary, secret).expect("write canary");

    let template = String::from_utf8(hostile("xxe-canary.xml")).expect("utf8 fixture");
    let source = template.replace("ARUNA_CANARY_PATH", &format!("file://{}", canary.display()));

    let (took, out) = run("root/CTH 5_XML_HFR/x.xml", source.as_bytes());
    assert!(took < BUDGET, "took {took:?}");
    assert!(
        !String::from_utf8_lossy(&out).contains(secret),
        "the canary reached the output"
    );
    assert!(
        String::from_utf8_lossy(&out).contains("&canary;"),
        "the reference should still be there, unresolved and literal"
    );
    // And the canary itself is untouched.
    assert_eq!(
        std::fs::read_to_string(&canary).expect("re-read"),
        secret,
        "the canary file was written to"
    );
}

#[test]
fn an_entity_pointing_at_the_network_is_never_fetched() {
    // 192.0.2.0/24 is RFC 5737 TEST-NET-1 and does not route. A connection
    // attempt takes seconds to give up; this must not take any.
    let (took, out) = run("root/CTH 5_XML_HFR/x.xml", &hostile("xxe-network.xml"));
    assert!(took < BUDGET, "took {took:?} — something tried to connect");
    assert!(String::from_utf8_lossy(&out).contains("192.0.2.1"));
}

#[test]
fn an_external_dtd_is_not_fetched() {
    let (took, out) = run("root/CTH 5_XML_HFR/x.xml", &hostile("external-dtd.xml"));
    assert!(took < BUDGET, "took {took:?} — something tried to connect");
    assert!(
        String::from_utf8_lossy(&out).contains("<!DOCTYPE"),
        "the declaration is part of the document and stays in it"
    );
}

#[test]
fn xinclude_is_not_followed() {
    let (took, out) = run(
        "root/CTH 5_XML_HFR/x.xml",
        &hostile("xinclude-external.xml"),
    );
    assert!(took < BUDGET, "took {took:?}");
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("xi:include"),
        "the element stays as an element"
    );
    assert!(
        !text.contains("root:x:0:0"),
        "/etc/passwd reached the output"
    );
}

#[test]
fn exponential_entity_expansion_stays_flat() {
    let source = hostile("billion-laughs.xml");
    let (took, out) = run("root/CTH 5_XML_HFR/x.xml", &source);
    assert!(took < BUDGET, "took {took:?}");
    // Expanded, this document is about a gigabyte. Normalising it may only
    // change the prologue, so the output is the input plus a declaration.
    assert!(
        out.len() < source.len() + 128,
        "output grew from {} to {} bytes",
        source.len(),
        out.len()
    );
}

#[test]
fn field_values_that_are_paths_stay_inside_the_package() {
    let dir = tempdir().expect("tempdir");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).expect("outside");
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    let archive = dir.path().join("corpus.zip");
    let mut zip = ZipWriter::new(std::fs::File::create(&archive).expect("create"));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("root/CTH 5_XML_HFR/traversal.xml", options)
        .expect("start");
    zip.write_all(&hostile("path-traversal-values.xml"))
        .expect("write");
    zip.finish().expect("finish");

    export::build(
        &archive,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    )
    .expect("builds");

    let mut written = Vec::new();
    let mut stack = vec![destination.clone()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                written.push(path);
            }
        }
    }
    assert!(
        std::fs::read_dir(&outside)
            .expect("read outside")
            .next()
            .is_none(),
        "something was written outside the destination"
    );
    for path in &written {
        let relative = path.strip_prefix(&destination).expect("under destination");
        // By component, not by substring: `..` escaped to `%2E%2E` is a name,
        // and the escaped form is exactly what is supposed to happen to it. The
        // written name is `_..%2F..%2F..%2F..%2Fetc%2Fpasswd.xml`, which is one
        // harmless component containing the characters and none of the meaning.
        for part in relative.components() {
            assert_ne!(
                part.as_os_str(),
                "..",
                "{relative:?} climbs out of the package"
            );
        }
        assert!(
            relative.starts_with(PACKAGE),
            "{relative:?} is outside the package folder"
        );
        // The package root carries the inventory and the manifest; everything
        // else is one group folder deep and no deeper.
        let depth = relative.components().count();
        assert!(
            depth == 2 || depth == 3,
            "{relative:?} is deeper than the package goes"
        );
    }
    assert!(
        written
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "xml")),
        "nothing was written at all, so nothing was proven"
    );
}

/// The three fixtures whose size is the point, built here rather than committed.
mod generated {
    use super::*;

    #[test]
    fn fifty_thousand_levels_of_nesting_do_not_take_the_stack_with_them() {
        let mut source = Vec::from(
            &b"<AOxml xmlns:AO=\"http://hethiter.net/ns/AO/1.0\"><AOHeader><docID>KBo 6.3</docID>\
               </AOHeader><body><text><l lg=\"Hit\"/>"[..],
        );
        for _ in 0..50_000 {
            source.extend_from_slice(b"<n>");
        }
        source.push(b'x');
        for _ in 0..50_000 {
            source.extend_from_slice(b"</n>");
        }
        source.extend_from_slice(b"</text></body></AOxml>");

        let (took, out) = run("root/CTH 5_XML_HFR/deep.xml", &source);
        assert!(took < BUDGET, "took {took:?}");
        assert_eq!(
            out.len(),
            source.len() + verify::DECLARATION.len(),
            "the body should be copied, not rewritten"
        );
    }

    #[test]
    fn a_hundred_thousand_attributes_are_read_in_bounded_time() {
        let mut source = Vec::from(
            &b"<AOxml xmlns:AO=\"http://hethiter.net/ns/AO/1.0\"><AOHeader><docID>KBo 6.4</docID>\
               </AOHeader><body><text><l lg=\"Hit\"/><w"[..],
        );
        for i in 0..100_000 {
            source.extend_from_slice(format!(" a{i}=\"v\"").as_bytes());
        }
        source.extend_from_slice(b"/></text></body></AOxml>");

        let (took, _) = run("root/CTH 5_XML_HFR/attrs.xml", &source);
        assert!(took < BUDGET, "took {took:?}");
    }

    #[test]
    fn an_eight_megabyte_text_node_is_copied_once_and_not_multiplied() {
        let mut source = Vec::from(
            &b"<AOxml xmlns:AO=\"http://hethiter.net/ns/AO/1.0\"><AOHeader><docID>KBo 6.5</docID>\
               </AOHeader><body><text><l lg=\"Hit\"/>"[..],
        );
        source.resize(source.len() + 8 * 1024 * 1024, b'x');
        source.extend_from_slice(b"</text></body></AOxml>");

        let (took, out) = run("root/CTH 5_XML_HFR/huge.xml", &source);
        assert!(took < BUDGET, "took {took:?}");
        assert_eq!(out.len(), source.len() + verify::DECLARATION.len());
    }
}
