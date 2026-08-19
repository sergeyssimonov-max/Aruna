//! The export, end to end, on an archive small enough to read.
//!
//! The real corpus is 71 MiB and is not in the repository, so until now the
//! only thing exercising the pipeline was a build against it — which meant the
//! pipeline could not be tested at all on a machine that did not have it, and
//! could not be tested at all by CI.
//!
//! This archive is four manuscripts and four pieces of debris, chosen so that
//! every awkward thing the corpus actually does happens here too: a siglum with
//! a slash in it, two different documents sharing a siglum inside one group,
//! the same siglum filed under two groups, a stylesheet instruction to drop,
//! and junk that the corpus's own gates have to keep out.

use aruna::export::{self, PACKAGE};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A manuscript as the corpus writes them: no declaration, a stylesheet
/// instruction, and the header the parser reads.
fn manuscript(siglum: &str) -> String {
    format!(
        r#"<?xml-stylesheet href="HPMxml.css" type="text/css"?><AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>  spacing  kept  </text></body></AOxml>"#
    )
}

fn write_archive(path: &Path) {
    let file = fs::File::create(path).expect("create archive");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let mut add = |name: &str, body: &str| {
        zip.start_file(name, options).expect("start");
        use std::io::Write as _;
        zip.write_all(body.as_bytes()).expect("write");
    };

    // Two groups. `KUB 2.1` is in both, which the corpus does 112 times.
    add("root/CTH 5_XML_HFR/KBo 1.1.xml", &manuscript("KBo 1.1"));
    add("root/CTH 5_XML_HFR/544-f.xml", &manuscript("544/f"));
    add("root/CTH 5_XML_TLH/KBo 1.1.xml", &manuscript("KBo 1.1"));
    add("root/CTH 9_XML_HFR/KUB 2.1.xml", &manuscript("KUB 2.1"));

    // Debris the gates must keep out, of every kind the archive carries.
    add("__MACOSX/root/CTH 5_XML_HFR/._KBo 1.1.xml", "resource fork");
    add("root/CTH 5_XML_HFR/.DS_Store", "finder");
    add("root/CTH 5_XML_HFR/HPMxml.css", "body { }");
    add(
        "root/CTH 5_XML_HFR/not-a-manuscript.xml",
        "<html><body>an encrypted blob would look like this</body></html>",
    );

    zip.finish().expect("finish");
}

fn built(dir: &Path) -> (export::Built, PathBuf) {
    let archive = dir.join("corpus.zip");
    write_archive(&archive);
    let built = export::build(&archive, dir, "test source").expect("the package builds");
    (built, dir.join(PACKAGE))
}

#[test]
fn the_package_holds_exactly_what_the_inventory_promises() {
    let dir = tempdir().expect("tempdir");
    let (built, root) = built(dir.path());

    assert_eq!(built.groups, 2, "CTH 5 and CTH 9");
    assert_eq!(built.documents, 4, "the debris is not a document");
    assert_eq!(built.group_links, 2);
    assert_eq!(built.fragment_links, 4);
    assert_eq!(built.disambiguated, 1, "the repeated KBo 1.1");
    assert_eq!(built.stylesheet_dropped, 4);

    // Structure: an inventory, two group directories, nothing else.
    let mut top: Vec<String> = fs::read_dir(&root)
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    top.sort();
    assert_eq!(
        top,
        vec!["CTH 5", "CTH 9", "TLHdig_Beta_0.3.html", "manifest.json"]
    );

    let mut group5: Vec<String> = fs::read_dir(root.join("CTH 5"))
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    group5.sort();
    assert_eq!(
        group5,
        vec![
            "544%2Ff.xml",
            "KBo 1.1 (CTH 5_XML_TLH).xml",
            "KBo 1.1.xml",
            "index.html",
        ],
        "the slash is escaped, the repeat has a place of its own, and the group has a page"
    );
}

/// Every link, checked against the filesystem — the same question the build
/// answers internally, asked again from outside it.
#[test]
fn every_link_resolves_and_opens_in_a_new_context() {
    let dir = tempdir().expect("tempdir");
    let (_, root) = built(dir.path());
    let html = fs::read_to_string(root.join(format!("{PACKAGE}.html"))).expect("inventory");

    let links = export::hrefs(&html);
    assert_eq!(links.len(), 6, "two groups and four fragments");

    for href in &links {
        let relative = export::resolve(href).unwrap_or_else(|| panic!("{href} is not relative"));
        let target = root.join(&relative);
        assert!(target.exists(), "{href} points at nothing");
        // Every link now names a file: a document, or the group's own page.
        // Safari shows nothing for a `file://` directory, so the package links
        // to pages rather than folders.
        assert!(target.is_file(), "{href} is not a file");
    }

    // Every anchor carries both attributes, and none of them is absolute.
    assert_eq!(html.matches("target=\"_blank\"").count(), links.len());
    assert_eq!(html.matches("rel=\"noopener\"").count(), links.len());
    assert!(!html.contains("file://"));
    assert!(!html.contains(&dir.path().display().to_string()));
}

#[test]
fn the_documents_in_the_package_are_the_normalised_ones() {
    let dir = tempdir().expect("tempdir");
    let (_, root) = built(dir.path());

    for name in [
        "CTH 5/KBo 1.1.xml",
        "CTH 5/544%2Ff.xml",
        "CTH 9/KUB 2.1.xml",
    ] {
        let text = fs::read_to_string(root.join(name)).expect(name);
        assert!(
            text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
            "{name} has no declaration"
        );
        assert!(
            !text.contains("xml-stylesheet"),
            "{name} kept a reference to a stylesheet the package does not ship"
        );
        // The body is scholarly text under xml:space="preserve".
        assert!(
            text.contains("  spacing  kept  "),
            "{name} had its whitespace edited"
        );
        assert!(text.contains("<docID>"), "{name} lost its header");
    }
}

/// The debris is what the corpus's own gates exist for, and none of it may
/// reach the package under any name.
#[test]
fn none_of_the_archive_debris_reaches_the_package() {
    let dir = tempdir().expect("tempdir");
    let (_, root) = built(dir.path());

    let mut files = Vec::new();
    collect(&root, &mut files);
    assert_eq!(
        files.len(),
        8,
        "four documents, one inventory, one manifest, two group pages: {files:?}"
    );
    for path in &files {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .to_string();
        assert!(
            name.ends_with(".xml")
                || name == format!("{PACKAGE}.html")
                || name == "manifest.json"
                || name == "index.html",
            "{name} is not something the package should hold"
        );
        assert!(!name.starts_with('.'), "{name} is hidden debris");
        assert!(!name.contains("not-a-manuscript"));
        assert!(!name.contains("HPMxml"));
    }
}

/// A second build over the first produces the same package, byte for byte.
#[test]
fn building_twice_gives_the_same_package() {
    let dir = tempdir().expect("tempdir");
    let (first, root) = built(dir.path());

    let mut before = Vec::new();
    collect(&root, &mut before);
    before.sort();
    let inventory_before = fs::read(root.join(format!("{PACKAGE}.html"))).expect("read");

    let second = export::build(&dir.path().join("corpus.zip"), dir.path(), "test source")
        .expect("rebuilds over itself");

    let mut after = Vec::new();
    collect(&root, &mut after);
    after.sort();

    assert_eq!(
        first, second,
        "the counts moved between two identical builds"
    );
    assert_eq!(before, after, "the file list moved");
    assert_eq!(
        inventory_before,
        fs::read(root.join(format!("{PACKAGE}.html"))).expect("read"),
        "the inventory is not reproducible"
    );
}

/// A destination holding someone else's work is not deleted to make room.
#[test]
fn a_destination_that_is_not_ours_stops_the_build() {
    let dir = tempdir().expect("tempdir");
    let archive = dir.path().join("corpus.zip");
    write_archive(&archive);

    let occupied = dir.path().join(PACKAGE);
    fs::create_dir_all(&occupied).expect("mkdir");
    fs::write(occupied.join("thesis.docx"), b"years of work").expect("write");

    let err = export::build(&archive, dir.path(), "test source").expect_err("must refuse");
    assert!(format!("{err}").contains("refusing to replace"), "{err}");
    assert!(
        occupied.join("thesis.docx").is_file(),
        "the refusal deleted the thing it was refusing to delete"
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read").flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A small archive carrying a very large entry is refused rather than read.
///
/// ZIP states an entry's uncompressed size and the reader believes it, so a few
/// hundred kilobytes on disk can be a few hundred megabytes in memory. Measured
/// before the limit existed: a 398 KiB archive took peak RSS to 834 MiB, which
/// is twice the document, because the raw bytes and the normalised copy are
/// both live while it is written.
///
/// A tenth of the limit is used here rather than the limit itself: the point is
/// that the read stops, and proving it with 64 MiB would be the same proof at
/// ten times the cost.
#[test]
fn one_enormous_document_does_not_take_the_memory_with_it() {
    let dir = tempdir().expect("tempdir");
    let archive = dir.path().join("bomb.zip");

    let file = fs::File::create(&archive).expect("create");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    {
        use std::io::Write as _;
        zip.start_file("root/CTH 1_XML_HFR/ok.xml", options)
            .expect("start");
        zip.write_all(manuscript("KBo 1.1").as_bytes())
            .expect("write");

        zip.start_file("root/CTH 1_XML_HFR/big.xml", options)
            .expect("start");
        // Compresses to almost nothing and inflates past the limit.
        zip.write_all(b"<AOxml><AOHeader><docID>BIG</docID></AOHeader><body>")
            .expect("write");
        let chunk = vec![b' '; 1024 * 1024];
        for _ in 0..(aruna::export::MAX_DOCUMENT / chunk.len() as u64 + 2) {
            zip.write_all(&chunk).expect("write");
        }
        zip.write_all(b"</body></AOxml>").expect("write");
    }
    zip.finish().expect("finish");

    let err = export::build(&archive, dir.path(), "test source").expect_err("must refuse");
    match err {
        aruna::error::ArunaError::ExportDocumentTooLarge { entry, limit } => {
            assert!(entry.ends_with("big.xml"), "names the entry: {entry}");
            assert_eq!(limit, aruna::export::MAX_DOCUMENT);
        }
        other => panic!("expected the size limit, got {other}"),
    }
    // A refused build publishes nothing.
    assert!(!dir.path().join(PACKAGE).exists());
}

/// A failed build leaves nothing behind.
///
/// The staging directory is where the package is assembled, and a failure
/// partway through used to leave it there — up to 372 MB of a package nobody
/// asked for, cleared only if the next build happened to use the same
/// destination.
#[test]
fn a_failed_build_takes_its_half_written_package_with_it() {
    let dir = tempdir().expect("tempdir");
    let archive = dir.path().join("bomb.zip");

    let file = fs::File::create(&archive).expect("create");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    {
        use std::io::Write as _;
        zip.start_file("root/CTH 1_XML_HFR/ok.xml", options)
            .expect("start");
        zip.write_all(manuscript("KBo 1.1").as_bytes())
            .expect("write");
        zip.start_file("root/CTH 1_XML_HFR/big.xml", options)
            .expect("start");
        zip.write_all(b"<AOxml><AOHeader><docID>BIG</docID></AOHeader><body>")
            .expect("write");
        let chunk = vec![b' '; 1024 * 1024];
        for _ in 0..(aruna::export::MAX_DOCUMENT / chunk.len() as u64 + 2) {
            zip.write_all(&chunk).expect("write");
        }
        zip.write_all(b"</body></AOxml>").expect("write");
    }
    zip.finish().expect("finish");

    assert!(export::build(&archive, dir.path(), "test source").is_err());

    let leftovers: Vec<String> = fs::read_dir(dir.path())
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name != "bomb.zip")
        .collect();
    assert!(
        leftovers.is_empty(),
        "the failed build left {leftovers:?} behind"
    );
}
