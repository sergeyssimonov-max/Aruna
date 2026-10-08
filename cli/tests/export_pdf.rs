//! The PDF phase of the export, on synthetic archives: both settings of the
//! switch, a document refused, an invariant broken, cancellation inside the
//! phase, and a build that is the same twice (owner's decisions of
//! 2026-09-30, questions 1, 3, 5 and 11).

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use aruna::export::{self, Pdf, PACKAGE};
use aruna::job::{Cancel, Job};
use aruna::pdf::Fonts;
use aruna::progress::{Event, Progress};
use support::{archive, manuscript};
use tempfile::tempdir;

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"))
            .expect("the fonts of the tree")
    })
}

/// A manuscript whose body is `body`.
fn with_body(siglum: &str, body: &str) -> String {
    manuscript(siglum, "FB", "2017-03-28").replace("<l lg=\"Hit\"/>text", body)
}

/// Five documents in three groups; one of them the model refuses (an
/// undeclared prefix), one carries a label and one the credit.
fn corpus(dir: &Path, extra: &[(&str, String)]) -> PathBuf {
    let mut entries = vec![
        (
            "root/CTH 5_XML_HFR/KBo 1.1.xml",
            manuscript("KBo 1.1", "FB", "2017-03-28"),
        ),
        (
            "root/CTH 5_XML_HFR/KBo 1.2.xml",
            with_body("KBo 1.2", "<lb/> a\u{100009}b <lb cu=\"\u{12000}\"/>"),
        ),
        (
            "root/CTH 9_XML_HFR/KUB 2.1.xml",
            with_body("KUB 2.1", "<lb/> \u{100000} a-na"),
        ),
        (
            "root/CTH 9_XML_HFR/KUB 2.2.xml",
            with_body("KUB 2.2", "<lb/> <x:y/> refused"),
        ),
        (
            "root/CTH 12_XML_HFR/IBoT 1.1.xml",
            manuscript("IBoT 1.1", "GM", "2019-01-02"),
        ),
    ];
    entries.extend(extra.iter().cloned());
    archive(&dir.join("corpus.zip"), &entries)
}

fn build(zip: &Path, out: &Path, pdf: Pdf<'_>) -> aruna::error::Result<export::Built> {
    std::fs::create_dir_all(out).expect("destination");
    export::build_with(zip, out, "test", pdf, &Job::unattended())
}

/// Every file of a tree, by path, with its bytes.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(root).expect("inside").to_path_buf();
                out.insert(rel, std::fs::read(&path).expect("readable"));
            }
        }
    }
    out
}

/// The inventory with the PDF links taken out, and the manifest with its
/// PDF entries taken out: what the package says apart from the PDFs.
fn without_pdfs(name: &Path, bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).into_owned();
    if name == Path::new(export::MANIFEST) {
        let mut out = String::new();
        let mut in_block = false;
        for line in text.lines() {
            if line == "  \"pdfs\": {" {
                in_block = true;
                continue;
            }
            if in_block {
                in_block = line != "  },";
                continue;
            }
            if line.trim_start().starts_with("\"pdf\": ") {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    } else {
        let mut out = text.clone();
        while let Some(at) = out.find(" <a href=\"./") {
            let end = out[at..].find("</a>").map(|e| at + e + 4).expect("closed");
            if out[at..end].ends_with(">PDF</a>") {
                out.replace_range(at..end, "");
            } else {
                break;
            }
        }
        out
    }
}

#[test]
fn the_switch_changes_nothing_but_the_pdfs_and_what_names_them() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), &[]);
    let off = build(&zip, &dir.path().join("off"), Pdf::Off).expect("builds without");
    let on = build(&zip, &dir.path().join("on"), Pdf::On(fonts())).expect("builds with");
    assert_eq!(off.pdfs, None);
    assert_eq!(
        on.pdfs,
        Some(export::PdfCount {
            built: 4,
            refused: 1
        })
    );
    assert_eq!((off.documents, off.groups), (on.documents, on.groups));

    let a = tree(&dir.path().join("off").join(PACKAGE));
    let b = tree(&dir.path().join("on").join(PACKAGE));
    let pdfs: Vec<&PathBuf> = b
        .keys()
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    assert_eq!(pdfs.len(), 4, "{pdfs:?}");
    for pdf in &pdfs {
        assert!(b[*pdf].starts_with(b"%PDF-"));
        assert!(
            a.contains_key(&pdf.with_extension("xml")),
            "a PDF beside no XML: {pdf:?}"
        );
    }
    assert!(
        !b.contains_key(Path::new("CTH 9/KUB 2.2.pdf")),
        "the refused one has a PDF"
    );
    for (path, bytes) in &a {
        let other = &b[path];
        if path.extension().is_some_and(|e| e == "html") || path == Path::new(export::MANIFEST) {
            assert_ne!(bytes, other, "{path:?} does not name the PDFs");
            assert_eq!(
                without_pdfs(path, other),
                String::from_utf8_lossy(bytes),
                "{path:?}"
            );
        } else {
            assert_eq!(bytes, other, "{path:?} changed with the PDFs");
        }
    }
    assert_eq!(b.len(), a.len() + 4);

    let manifest = String::from_utf8_lossy(&b[Path::new(export::MANIFEST)]).into_owned();
    assert!(
        manifest.contains("\"pdf\": \"CTH 5/KBo 1.2.pdf\""),
        "{manifest}"
    );
    assert!(
        manifest.contains("\"pdf\": { \"refused\": \"the document model: "),
        "{manifest}"
    );
    assert!(manifest.contains("\"template\": \"aruna-pdf-template 1\""));
    let off_manifest = String::from_utf8_lossy(&a[Path::new(export::MANIFEST)]).into_owned();
    assert!(
        !off_manifest.contains("\"pdf\""),
        "a build without PDFs names one"
    );
    let inventory =
        String::from_utf8_lossy(&b[&PathBuf::from(format!("{PACKAGE}.html"))]).into_owned();
    assert!(
        inventory.contains("href=\"./CTH%205/KBo%201.2.pdf\""),
        "no link beside the XML"
    );
    assert!(
        !inventory.contains("KUB%202.2.pdf"),
        "a link to a PDF that is not there"
    );
}

/// **Two repeated clusters in a row refuse this document only** (owner's
/// decision of 2026-10-02): the wrappers of variant A would overlap, and the
/// document gets no PDF and a record in the manifest, while the build goes on
/// and the others get theirs. Until that day this stopped the whole build as
/// a broken invariant – the name of the test is kept.
#[test]
fn a_broken_invariant_stops_the_build_and_names_the_document() {
    let dir = tempdir().expect("tempdir");
    // Two repeated clusters side by side (no corpus document has it).
    let zip = corpus(
        dir.path(),
        &[(
            "root/CTH 12_XML_HFR/IBoT 1.3.xml",
            with_body("IBoT 1.3", "<lb/> \u{160}\u{303}\u{160}\u{303}"),
        )],
    );
    let out = dir.path().join("out");
    let built = build(&zip, &out, Pdf::On(fonts())).expect("the build goes on");
    assert_eq!(
        built.pdfs,
        Some(export::PdfCount {
            built: 4,
            refused: 2
        })
    );
    let root = out.join(PACKAGE);
    assert!(!root.join("CTH 12/IBoT 1.3.pdf").exists(), "a PDF of it");
    assert!(
        root.join("CTH 12/IBoT 1.3.xml").is_file(),
        "its XML is gone"
    );
    assert!(
        root.join("CTH 12/IBoT 1.1.pdf").is_file(),
        "the others lost theirs"
    );
    let manifest = std::fs::read_to_string(root.join(export::MANIFEST)).expect("manifest");
    let at = manifest
        .find("\"file\": \"CTH 12/IBoT 1.3.xml\"")
        .expect("its entry");
    let entry = &manifest[at..at + manifest[at..].find('}').expect("closed")];
    assert!(
        entry.contains("\"pdf\": { \"refused\": \"page 1: two repeated clusters overlap at MCID"),
        "{entry}"
    );
}

/// **One repeated cluster, or two apart, is wrapped and the PDF is built** –
/// the decision of 2026-10-02 refuses only two in a row.
#[test]
fn one_repeated_cluster_or_two_apart_still_get_their_pdf() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(
        dir.path(),
        &[
            (
                "root/CTH 12_XML_HFR/IBoT 1.4.xml",
                with_body("IBoT 1.4", "<lb/> \u{160}\u{303}"),
            ),
            (
                "root/CTH 12_XML_HFR/IBoT 1.5.xml",
                with_body("IBoT 1.5", "<lb/> \u{160}\u{303} a \u{160}\u{303}"),
            ),
        ],
    );
    let out = dir.path().join("out");
    let built = build(&zip, &out, Pdf::On(fonts())).expect("builds");
    assert_eq!(
        built.pdfs,
        Some(export::PdfCount {
            built: 6,
            refused: 1
        })
    );
    let root = out.join(PACKAGE);
    for pdf in ["CTH 12/IBoT 1.4.pdf", "CTH 12/IBoT 1.5.pdf"] {
        let bytes = std::fs::read(root.join(pdf)).expect("the PDF is there");
        assert!(bytes.starts_with(b"%PDF-"), "{pdf}");
    }
}

#[test]
fn two_builds_are_the_same_package() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), &[]);
    build(&zip, &dir.path().join("a"), Pdf::On(fonts())).expect("builds");
    build(&zip, &dir.path().join("b"), Pdf::On(fonts())).expect("builds");
    assert!(
        tree(&dir.path().join("a").join(PACKAGE)) == tree(&dir.path().join("b").join(PACKAGE)),
        "two builds differ"
    );
}

/// Stops the run the first time the PDF phase reports.
struct StopInPdfs<'a>(&'a Cancel, std::sync::Mutex<Vec<String>>);

impl Progress for StopInPdfs<'_> {
    fn report(&self, event: Event<'_>) {
        let name = match event {
            Event::WritingPdfs { .. } => "WritingPdfs",
            Event::PdfsWritten { .. } => "PdfsWritten",
            Event::CheckingPackage => "CheckingPackage",
            _ => "other",
        };
        self.1.lock().expect("unpoisoned").push(name.into());
        if name == "WritingPdfs" {
            self.0.cancel();
        }
    }
}

#[test]
fn a_run_stopped_in_the_pdf_phase_publishes_nothing() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), &[]);
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).expect("destination");
    let cancel = Cancel::new();
    let sink = StopInPdfs(&cancel, std::sync::Mutex::new(Vec::new()));
    let err = export::build_with(
        &zip,
        &out,
        "test",
        Pdf::On(fonts()),
        &Job::new(&sink, &cancel),
    )
    .expect_err("cancelled");
    assert!(
        matches!(err, aruna::error::ArunaError::Cancelled { .. }),
        "{err}"
    );
    assert_eq!(std::fs::read_dir(&out).expect("readable").count(), 0);
    let seen = sink.1.lock().expect("unpoisoned").clone();
    assert!(seen.contains(&"WritingPdfs".to_string()));
    assert!(!seen.contains(&"CheckingPackage".to_string()), "{seen:?}");
}

/// The tick says how far the phase is and ends on the whole.
#[test]
fn the_pdf_phase_reports_its_progress_to_the_end() {
    struct Ticks(std::sync::Mutex<Vec<(usize, usize, usize)>>);
    impl Progress for Ticks {
        fn report(&self, event: Event<'_>) {
            if let Event::PdfsWritten { done, built, total } = event {
                assert!(event.is_tick());
                self.0
                    .lock()
                    .expect("unpoisoned")
                    .push((done, built, total));
            }
        }
    }
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), &[]);
    let out = dir.path().join("out");
    std::fs::create_dir_all(&out).expect("destination");
    let ticks = Ticks(std::sync::Mutex::new(Vec::new()));
    let cancel = Cancel::new();
    export::build_with(
        &zip,
        &out,
        "test",
        Pdf::On(fonts()),
        &Job::new(&ticks, &cancel),
    )
    .expect("builds");
    assert_eq!(ticks.0.lock().expect("unpoisoned").last(), Some(&(5, 4, 5)));
}

/// **Authenticity, option 2 of `XML-CONTRACT.md` §4** (owner's decision of
/// 2026-10-06): every PDF stands in the folder of its XML, the manifest names
/// the pair, and the PDF itself carries no copy of the source – no embedded
/// file, no digest of it – only the constant fields of decision 13, which
/// `pdf_module::the_file_carries_no_date_and_builds_the_same_twice` holds.
/// Without PDFs the manifest and the inventory name none
/// (`the_switch_changes_nothing_but_the_pdfs_and_what_names_them`).
///
/// The SHA-256 of the source that the decision also puts in the manifest is
/// not written by the export today, and this test does not claim it: found on
/// 2026-10-08, left to the owner before 2.7.0 (specification 7.3).
#[test]
fn each_pdf_is_paired_with_its_xml_and_carries_no_copy_of_it() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), &[]);
    let out = dir.path().join("on");
    build(&zip, &out, Pdf::On(fonts())).expect("builds");
    let root = out.join(PACKAGE);
    let files = tree(&root);
    let manifest = String::from_utf8_lossy(&files[Path::new(export::MANIFEST)]).into_owned();

    let pdfs: Vec<&PathBuf> = files
        .keys()
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    assert_eq!(pdfs.len(), 4, "{pdfs:?}");
    for pdf in pdfs {
        let xml = pdf.with_extension("xml");
        assert!(files.contains_key(&xml), "{pdf:?} without its XML");
        assert_eq!(pdf.parent(), xml.parent(), "{pdf:?}");

        // The manifest's entry of this document: its "file" is the XML, its
        // "pdf" the PDF, in the same entry.
        let named = format!("\"pdf\": \"{}\"", pdf.to_string_lossy());
        let at = manifest.find(&named).expect("the manifest names the PDF");
        let entry = &manifest[manifest[..at].rfind('{').expect("an entry")..at];
        assert!(
            entry.contains(&format!("\"file\": \"{}\"", xml.to_string_lossy())),
            "{entry}"
        );

        // No copy of the source in the PDF: no embedded file, no digest of
        // the XML, raw or in any decoded stream.
        let bytes = &files[pdf];
        let source = &files[&xml];
        let digest = aruna::sha256::sha256_hex(source);
        let doc = lopdf::Document::load_mem(bytes).expect("readable");
        let mut texts = vec![bytes.clone()];
        for object in doc.objects.values() {
            if let Ok(stream) = object.as_stream() {
                texts.push(
                    stream
                        .decompressed_content()
                        .unwrap_or_else(|_| stream.content.clone()),
                );
            }
        }
        for text in &texts {
            for needle in [
                b"/EmbeddedFile".as_slice(),
                b"/Filespec",
                digest.as_bytes(),
                digest.to_uppercase().as_bytes(),
                b"<AOxml",
            ] {
                assert!(
                    memchr::memmem::find(text, needle).is_none(),
                    "{pdf:?} carries {}",
                    String::from_utf8_lossy(needle)
                );
            }
        }
    }
}
