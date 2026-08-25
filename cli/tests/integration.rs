//! Integration tests for the Aruna pipeline (offline ZIP fixtures).

use aruna::archive::parse_zip;
use aruna::error::ArunaError;
use aruna::html::{escape_html, render_html};
use aruna::parse::{parse_manuscript, ManuscriptRecord, MISSING};
use aruna::paths::{ensure_output_parent, OUTPUT_FILE_NAME};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn make_zip(path: &Path, files: &[(&str, &str)]) {
    let f = File::create(path).expect("create zip");
    let mut zw = ZipWriter::new(f);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in files {
        zw.start_file(*name, opts).expect("start");
        zw.write_all(body.as_bytes()).expect("write");
    }
    zw.finish().expect("finish");
}

#[test]
fn end_to_end_zip_to_html() {
    let dir = tempdir().unwrap();
    let zip_path = dir.path().join("sample.zip");
    make_zip(
        &zip_path,
        &[
            (
                "TLH/CTH 786_XML_HFR/KBo 17.86+.xml",
                r#"<AOxml><AOHeader><docID>KBo 17.86+</docID>
                <neu><uebern editor="FB" date="2017-03-28"/></neu>
                </AOHeader></AOxml>"#,
            ),
            (
                "TLH/CTH 17_XML_HAnn/KUB 23.117.xml",
                r#"<AOxml><AOHeader><docID>KUB 23.117</docID>
                <creation-date date="2021-09-27T15:06:47"/>
                </AOHeader></AOxml>"#,
            ),
            (
                "TLH/CTH 249_XML_PTAC/evil.xml",
                r#"<AOxml><AOHeader><docID>Evil & Co <x></docID>
                <annot editor="James "J" Burgin" date="2024-11-12"/>
                </AOHeader></AOxml>"#,
            ),
        ],
    );

    let records = parse_zip(&zip_path, &aruna::job::Job::unattended()).expect("parse");
    assert_eq!(records.len(), 3);

    let html = render_html(&records, "test source", "2026-01-01 00:00:00 UTC");
    assert!(html.contains("Thesaurus Linguarum Hethaeorum Digitalis"));
    assert!(html.contains("KBo 17.86+"));
    assert!(html.contains("CTH 786"));
    assert!(html.contains("FB"));
    assert!(html.contains("2017"));
    // Escaping
    assert!(html.contains("<") || html.contains("Evil"));
    assert!(!html.contains("<x>"));

    let out = dir.path().join("Downloads").join(OUTPUT_FILE_NAME);
    ensure_output_parent(&out).unwrap();
    fs::write(&out, html.as_bytes()).unwrap();
    assert!(out.is_file());
    let written = fs::read_to_string(&out).unwrap();
    assert!(written.starts_with("<!DOCTYPE html>"));
}

#[test]
fn corrupted_and_empty_archives() {
    let dir = tempdir().unwrap();

    let bad = dir.path().join("bad.zip");
    fs::write(&bad, b"PK\x03\x04garbage").unwrap();
    assert!(matches!(
        parse_zip(&bad, &aruna::job::Job::unattended()),
        Err(ArunaError::Zip(_))
    ));

    let empty = dir.path().join("empty.zip");
    make_zip(&empty, &[]);
    // Empty central directory may be EmptyArchive or Zip depending on crate
    assert!(parse_zip(&empty, &aruna::job::Job::unattended()).is_err());

    let noxml = dir.path().join("noxml.zip");
    make_zip(&noxml, &[("a.txt", "hi"), ("b.md", "#")]);
    assert!(matches!(
        parse_zip(&noxml, &aruna::job::Job::unattended()),
        Err(ArunaError::EmptyArchive)
    ));
}

#[test]
fn malformed_xml_variants() {
    let cases: Vec<(&str, &str)> = vec![
        ("no header", "<root/>"),
        ("unclosed", "<AOHeader><docID>X"),
        ("only path", ""),
        (
            "broken attrs",
            r#"<AOHeader><docID>Y</docID><uebern editor=FB date=2017/></AOHeader>"#,
        ),
    ];
    for (label, xml) in cases {
        let r = parse_manuscript(&format!("CTH 9_XML/{label}.xml"), xml);
        // Must never panic; title should at least carry CTH or stem
        assert!(
            r.title.contains("CTH 9") || !r.title.is_empty(),
            "label={label} title={}",
            r.title
        );
        assert!(!r.authorship.is_empty());
        assert!(!r.year.is_empty());
    }
}

#[test]
fn html_escape_roundtrip_safety() {
    let rec = ManuscriptRecord {
        title: "<img onerror=alert(1) src=x>".into(),
        sigla: "<img onerror=alert(1) src=x>".into(),
        cth: None,
        cth_num: u32::MAX,
        authorship: "A&B".into(),
        year: "\"2020\"".into(),
        lang: "Hit".into(),
        inv: "—".into(),
        corpus: "HFR".into(),
    };
    let html = render_html(&[rec], "s<>", "t&");
    assert!(!html.contains("<img onerror"));
    assert!(html.contains("&lt;img"));
    assert!(html.contains("A&amp;B"));
    assert_eq!(escape_html("—"), "—");
    assert_eq!(MISSING, "—");
}

#[test]
fn run_with_local_zip_writes_downloads() {
    // Override HOME so dirs::download_dir / home_dir resolve under temp
    let dir = tempdir().unwrap();
    let home = dir.path().join("home");
    let downloads = home.join("Downloads");
    fs::create_dir_all(&downloads).unwrap();

    // SAFETY: test-only env mutation in serial cargo test process for this test
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_DOWNLOAD_DIR", &downloads);

    let zip_path = dir.path().join("mini.zip");
    make_zip(
        &zip_path,
        &[(
            "CTH 1_XML/A.xml",
            r#"<AOHeader><docID>Tablet A</docID><uebern editor="ZZ" date="2015-06-01"/></AOHeader>"#,
        )],
    );

    let out = aruna::run(Some(&zip_path), &aruna::job::Job::unattended()).expect("run");
    assert!(out.ends_with(OUTPUT_FILE_NAME));
    assert!(out.exists());
    let body = fs::read_to_string(&out).unwrap();
    assert!(body.contains("Tablet A"));
    assert!(body.contains("ZZ"));
    assert!(body.contains("2015"));
}

/// Where the full TLHdig archive lives, when it lives anywhere.
///
/// `ARUNA_ZIP` — the variable the binary itself honours — is read first, then
/// `ARUNA_FIXTURE_ZIP`, so CI can keep the 71 MiB download in a cache directory
/// outside the checkout. `tests/corpus.rs` reads the same pair, in the same
/// order: one archive, named the same way wherever it is looked for. Otherwise it is `cli/fixtures/`, anchored to
/// `CARGO_MANIFEST_DIR` rather than written relative: cargo happens to run tests
/// from the package root, so a bare `fixtures/...` works today, but it is a
/// property of the runner rather than of this test, and it silently produced a
/// skip rather than an error when it did not hold.
fn fixture_path() -> std::path::PathBuf {
    for name in ["ARUNA_ZIP", "ARUNA_FIXTURE_ZIP"] {
        if let Some(p) = std::env::var_os(name) {
            return std::path::PathBuf::from(p);
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("TLHbasisONLINE25_1_ZENODO_Beta_03.zip")
}

/// The whole corpus, parsed: 24 000 manuscripts of real cuneiform.
///
/// This is the test that catches what handmade fixtures cannot — the character
/// boundary panic on multi-byte transliterations came from here. It needs the
/// archive, which is 71 MiB and not in the repository, so it skips when the file
/// is absent.
///
/// That skip used to be invisible: `eprintln!` is swallowed without
/// `--nocapture`, so the test reported "ok" having asserted nothing, and in CI
/// — where the archive never exists — it had never once run. Setting
/// `ARUNA_REQUIRE_FIXTURE=1` turns a missing archive into a failure, so a job
/// that means to exercise the corpus cannot quietly stop doing so.
#[test]
fn real_fixture_zip_if_present() {
    let fixture = fixture_path();
    if !fixture.is_file() {
        assert!(
            std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
            "ARUNA_REQUIRE_FIXTURE is set but {} is missing",
            fixture.display()
        );
        eprintln!("skipping corpus test — {} not present", fixture.display());
        return;
    }

    let records = parse_zip(&fixture, &aruna::job::Job::unattended()).expect("fixture parse");
    assert!(
        records.len() > 20_000,
        "expected the full corpus, got {}",
        records.len()
    );

    // Every record must be renderable: the table prints these fields directly,
    // and an empty one would show as a blank cell rather than the missing-value
    // dash. Checked across all 24k, not a sample — the failures this test exists
    // to catch live in the rare documents.
    for (i, r) in records.iter().enumerate() {
        assert!(!r.title.is_empty(), "record {i} has no title");
        assert!(!r.authorship.is_empty(), "record {i} has no authorship");
        assert!(!r.year.is_empty(), "record {i} has no year");
        assert!(!r.sigla.is_empty(), "record {i} has no sigla");
    }

    report_coverage(&records);

    let html = render_html(&records, "fixture", "now");
    assert!(html.contains("<tbody>"));
    // Rendering the whole corpus is the other half: escaping runs over every
    // field, and a panic there would reach the user as a failed run.
    assert!(
        html.len() > 1_000_000,
        "the full corpus should render a large document, got {} bytes",
        html.len()
    );
}

/// How much of each column the corpus actually fills — printed, and floored.
///
/// A field that is present in the document but not in the record shows as the
/// missing-value dash, exactly like a field the corpus never recorded, and
/// nothing distinguished the two. The editor column was 62% dashes for months;
/// measured against the archive, 549 of those documents did name a person, in
/// an `author=` attribute the parser did not read. The rest — 14 349 — name
/// nobody anywhere in the file, which is a fact about TLHdig and not about
/// this program.
///
/// The floors below are what the corpus supports today, a little under the
/// measured values. They are a tripwire rather than a target: a drop means the
/// parser stopped seeing something it used to see, and it is worth finding out
/// which before shipping a table full of dashes.
///
/// Measured on TLHdig Beta 0.3 (23 936 manuscripts):
/// editor 40.1%, year 100%, lang 99.1%, corpus 100%, inventory number 11.6%.
fn report_coverage(records: &[ManuscriptRecord]) {
    let total = records.len();
    let share = |filled: usize| 100.0 * filled as f64 / total as f64;
    let count =
        |f: fn(&ManuscriptRecord) -> &str| records.iter().filter(|r| f(r) != MISSING).count();

    let editor = count(|r| &r.authorship);
    let year = count(|r| &r.year);
    let lang = count(|r| &r.lang);
    let corpus = count(|r| &r.corpus);
    let inv = count(|r| &r.inv);

    eprintln!("corpus coverage over {total} manuscripts:");
    for (name, filled) in [
        ("editor", editor),
        ("year", year),
        ("lang", lang),
        ("corpus", corpus),
        ("inv", inv),
    ] {
        eprintln!("  {name:<8} {filled:>6}  {:.1}%", share(filled));
    }

    for (name, filled, floor) in [
        ("editor", editor, 39.0),
        ("year", year, 99.0),
        ("lang", lang, 98.0),
        ("corpus", corpus, 99.0),
        ("inv", inv, 10.0),
    ] {
        assert!(
            share(filled) >= floor,
            "{name} is filled for {:.1}% of the corpus, below the {floor}% this archive supports \
             — the parser has stopped reading something it used to read",
            share(filled)
        );
    }
}
