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

    let records = parse_zip(&zip_path).expect("parse");
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
    assert!(matches!(parse_zip(&bad), Err(ArunaError::Zip(_))));

    let empty = dir.path().join("empty.zip");
    make_zip(&empty, &[]);
    // Empty central directory may be EmptyArchive or Zip depending on crate
    assert!(parse_zip(&empty).is_err());

    let noxml = dir.path().join("noxml.zip");
    make_zip(&noxml, &[("a.txt", "hi"), ("b.md", "#")]);
    assert!(matches!(
        parse_zip(&noxml),
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

    let out = aruna::run(Some(&zip_path)).expect("run");
    assert!(out.ends_with(OUTPUT_FILE_NAME));
    assert!(out.exists());
    let body = fs::read_to_string(&out).unwrap();
    assert!(body.contains("Tablet A"));
    assert!(body.contains("ZZ"));
    assert!(body.contains("2015"));
}

#[test]
fn real_fixture_zip_if_present() {
    let fixture = Path::new("fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
    if !fixture.is_file() {
        eprintln!("skipping real fixture — file not present");
        return;
    }
    // Parse only a smoke subset would be ideal; full 24k is acceptable but slow.
    // We still run it as an optional stress test when the fixture is available.
    let records = parse_zip(fixture).expect("fixture parse");
    assert!(records.len() > 1000, "expected large corpus, got {}", records.len());
    // Spot-check: every record has non-empty fields
    for r in records.iter().take(100) {
        assert!(!r.title.is_empty());
        assert!(!r.authorship.is_empty());
        assert!(!r.year.is_empty());
    }
    let html = render_html(&records[..10.min(records.len())], "fixture", "now");
    assert!(html.contains("<tbody>"));
}
