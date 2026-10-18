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
//! **a document carrying two of them**, and junk that the corpus's own gates
//! have to keep out.

mod support;

use aruna::export::{self, PACKAGE};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A manuscript as the corpus writes them: no declaration, a stylesheet
/// instruction, and the header the parser reads.
fn manuscript(siglum: &str) -> String {
    stylesheets(1, siglum)
}

/// The same, with `count` copies of the stylesheet instruction in front.
///
/// One document of TLHdig Beta 0.3 carries two —
/// `CTH 475_XML_HFR/KBo 43.119.xml`, byte-identical instructions, both in the
/// prologue and both removed. It is the whole reason this helper exists: with
/// one instruction per document a count of documents and a count of
/// instructions are the same number, and the difference between them cannot be
/// tested at all.
fn stylesheets(count: usize, siglum: &str) -> String {
    let pi = r#"<?xml-stylesheet href="HPMxml.css" type="text/css"?>"#.repeat(count);
    format!(
        r#"{pi}<AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>  spacing  kept  </text></body></AOxml>"#
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
    // Two stylesheet instructions, as `CTH 475_XML_HFR/KBo 43.119.xml` has.
    add("root/CTH 9_XML_HFR/KUB 2.1.xml", &stylesheets(2, "KUB 2.1"));

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
    let built = export::build(
        &archive,
        dir,
        "test source",
        support::fonts(),
        &aruna::job::Job::unattended(),
    )
    .expect("the package builds");
    (built, dir.join(PACKAGE))
}

#[test]
fn the_package_holds_exactly_what_the_inventory_promises() {
    let dir = tempdir().expect("tempdir");
    let (built, root) = built(dir.path());

    assert_eq!(built.groups, 2, "CTH 5 and CTH 9");
    assert_eq!(built.documents, 4, "the debris is not a document");
    assert_eq!(built.document_links, 4, "one link per document");
    assert_eq!(built.disambiguated, 1, "the repeated KBo 1.1");
    // **Instructions, not documents.** Four documents carry a stylesheet
    // instruction and one of them carries two, so five are removed. Until
    // 2026-09-10 this counted documents while every label in the tree —
    // `export_beta` prints "stylesheet PIs dropped" — said instructions, and on
    // the corpus the two answers differ by exactly one.
    assert_eq!(built.stylesheet_dropped, 5);

    // Structure: an inventory, the font it draws `U+100000` with, that font's
    // terms, the terms of the CTH titles the inventory shows, the manifest, two
    // group directories, nothing else.
    let mut top: Vec<String> = fs::read_dir(&root)
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    top.sort();
    assert_eq!(
        top,
        vec![
            "CTH 5",
            "CTH 9",
            "CTH-TITLES-TERMS.txt",
            "TLHdig_Beta_0.3.html",
            "UllikummiA-TERMS.txt",
            "UllikummiA.ttf",
            "manifest.json"
        ]
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
            "544%2Ff.pdf",
            "544%2Ff.xml",
            "KBo 1.1 (CTH 5_XML_TLH).pdf",
            "KBo 1.1 (CTH 5_XML_TLH).xml",
            "KBo 1.1.pdf",
            "KBo 1.1.xml",
        ],
        "the slash is escaped, the repeat has a place of its own, and the \
         folder holds documents and the PDF of each, in pairs by name, and \
         nothing else"
    );
}

/// Every link, checked against the filesystem — the same question the build
/// answers internally, asked again from outside it.
#[test]
fn every_link_resolves_and_opens_in_a_new_context() {
    let dir = tempdir().expect("tempdir");
    let (_, root) = built(dir.path());
    let html = fs::read_to_string(root.join(format!("{PACKAGE}.html"))).expect("inventory");

    // Was 8 – four fragments and the PDF of each – until 2026-10-19; now one
    // link per document, at its PDF (owner's decision of that day). Every
    // document here gets a PDF, so every link is one.
    let links = export::hrefs(&html);
    assert_eq!(
        links.len(),
        4,
        "four fragments, one link each, and nothing else"
    );

    for href in &links {
        let relative = export::resolve(href).unwrap_or_else(|| panic!("{href} is not relative"));
        let target = root.join(&relative);
        assert!(target.exists(), "{href} points at nothing");
        // Every link names a manuscript. Never a folder — Safari shows nothing
        // for a `file://` directory — and never a page about a folder, which is
        // what the CTH index pages were until they were given up.
        assert!(target.is_file(), "{href} is not a file");
        // At its PDF, and the manuscript the PDF belongs to is in the package.
        assert!(
            href.ends_with(".pdf"),
            "{href} is not the PDF of a manuscript, so the inventory links something it should not"
        );
        assert!(
            target.with_extension("xml").is_file(),
            "{href} is the PDF of no manuscript in the package"
        );
    }
    let distinct: std::collections::BTreeSet<&&str> = links.iter().collect();
    assert_eq!(distinct.len(), 4, "a manuscript linked twice: {links:?}");

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
        13,
        "four documents and the PDF of each, one inventory, one manifest, the font and its terms, the CTH titles' terms: {files:?}"
    );
    for path in &files {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .to_string();
        let pdf_of_a_document =
            name.ends_with(".pdf") && files.contains(&path.with_extension("xml"));
        assert!(
            name.ends_with(".xml")
                || pdf_of_a_document
                || name == format!("{PACKAGE}.html")
                || name == "manifest.json"
                || name == aruna::fonts::PACKAGED_FONT
                || name == aruna::fonts::PACKAGED_TERMS
                || name == aruna::cth_titles::PACKAGED_TERMS,
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

    let second = export::build(
        &dir.path().join("corpus.zip"),
        dir.path(),
        "test source",
        support::fonts(),
        &aruna::job::Job::unattended(),
    )
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

    let err = export::build(
        &archive,
        dir.path(),
        "test source",
        support::fonts(),
        &aruna::job::Job::unattended(),
    )
    .expect_err("must refuse");
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

    let err = export::build(
        &archive,
        dir.path(),
        "test source",
        support::fonts(),
        &aruna::job::Job::unattended(),
    )
    .expect_err("must refuse");
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

    assert!(export::build(
        &archive,
        dir.path(),
        "test source",
        support::fonts(),
        &aruna::job::Job::unattended()
    )
    .is_err());

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

/// **Schema 3: the run and the two digests of every document** (owner's
/// decision of 2026-10-08, `XML-CONTRACT.md` §4). The run
/// identifier is computed again from the archive's MD5 and the two versions;
/// each document's `sha256` is that of the file in the package, and its
/// `source_sha256` that of the archive entry it was normalised from – found
/// here by normalising every entry again, not by asking the export.
#[test]
fn the_manifest_names_the_run_and_the_two_digests_of_every_document() {
    use aruna::export::manifest::{run_id, values_of};
    use aruna::sha256::sha256_hex;
    use std::io::Read as _;

    let dir = tempdir().expect("tempdir");
    let (_, root) = built(dir.path());
    let manifest = fs::read_to_string(root.join(export::MANIFEST)).expect("manifest");
    assert!(manifest.contains("\"schema\": 3,"), "{manifest}");
    let md5 = aruna::md5::md5_file(&dir.path().join("corpus.zip")).expect("md5");
    let id = run_id(
        &md5,
        env!("CARGO_PKG_VERSION"),
        aruna::pdf::TEMPLATE_VERSION,
    );
    assert!(
        manifest.contains(&format!("  \"run\": {{\n    \"id\": \"{id}\",")),
        "{manifest}"
    );

    // Every entry of the archive, normalised: what each would be in the
    // package, and the digest of what the archive gave.
    let mut zip =
        zip::ZipArchive::new(fs::File::open(dir.path().join("corpus.zip")).expect("archive"))
            .expect("readable");
    let mut sources: Vec<(Vec<u8>, String)> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read");
        sources.push((export::normalize_document(&bytes), sha256_hex(&bytes)));
    }

    let groups = &manifest[manifest.find("\"groups\": [").expect("groups")..];
    let files = values_of(groups, "file");
    let written = values_of(groups, "sha256");
    let read = values_of(groups, "source_sha256");
    assert_eq!(files.len(), 4);
    assert_eq!((written.len(), read.len()), (4, 4), "{groups}");
    for ((file, sha), source) in files.iter().zip(&written).zip(&read) {
        let bytes = fs::read(root.join(file)).expect(file);
        assert_eq!(sha, &sha256_hex(&bytes), "{file}");
        let from = sources
            .iter()
            .find(|(normalised, _)| normalised == &bytes)
            .map(|(_, digest)| digest)
            .expect("an archive entry normalises to it");
        assert_eq!(source, from, "{file}");
        assert_ne!(sha, source, "{file}: normalisation changed its bytes");
    }
}

/// Bit 11 of the general purpose flag – «the name is UTF-8» – cleared in
/// every local and central header of a small archive, as the corpus archive
/// has it on none of its 26 910 entries.
fn clear_the_utf8_flag(zip: &Path) {
    let mut bytes = fs::read(zip).expect("archive");
    let u16_at = |b: &[u8], at: usize| u16::from_le_bytes([b[at], b[at + 1]]) as usize;
    let u32_at =
        |b: &[u8], at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize;
    let end = bytes.len() - 22;
    assert_eq!(
        &bytes[end..end + 4],
        b"PK\x05\x06",
        "no comment, end record last"
    );
    let (entries, mut at) = (u16_at(&bytes, end + 10), u32_at(&bytes, end + 16));
    for _ in 0..entries {
        assert_eq!(&bytes[at..at + 4], b"PK\x01\x02");
        bytes[at + 9] &= !0x08;
        let local = u32_at(&bytes, at + 42);
        assert_eq!(&bytes[local..local + 4], b"PK\x03\x04");
        bytes[local + 7] &= !0x08;
        at += 46 + u16_at(&bytes, at + 28) + u16_at(&bytes, at + 30) + u16_at(&bytes, at + 32);
    }
    fs::write(zip, bytes).expect("archive");
}

/// **Each document names the archive entry it was read from, byte for byte**
/// (owner's decision of 2026-10-11, `XML-CONTRACT.md` §4): `source_entry` is
/// the name in the zip's catalogue – no Unicode normalisation, `/` between
/// components – of the entry whose bytes `source_sha256` is the digest of.
/// Here `München` in NFD, as macOS writes it, with the UTF-8 flag of the
/// archive and without it: `zip` reads an unflagged name as CP437, and the
/// manifest must still carry the bytes.
#[test]
fn each_document_names_its_archive_entry_byte_for_byte() {
    use aruna::export::manifest::values_of;
    use aruna::sha256::sha256_hex;
    use std::io::{Read as _, Write as _};

    let nfd = "root/CTH 5_XML_HFR/Mu\u{308}nchen 5.xml";
    let nfc = "root/CTH 5_XML_HFR/M\u{fc}nchen 5.xml";
    for flagged in [true, false] {
        let dir = tempdir().expect("tempdir");
        let zip = dir.path().join("corpus.zip");
        let mut writer = ZipWriter::new(fs::File::create(&zip).expect("archive"));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, siglum) in [
            (nfd, "München 5"),
            ("root/CTH 9_XML_HFR/KUB 2.1.xml", "KUB 2.1"),
        ] {
            writer.start_file(name, options).expect("entry");
            writer
                .write_all(manuscript(siglum).as_bytes())
                .expect("write");
        }
        writer.finish().expect("finish");
        if !flagged {
            clear_the_utf8_flag(&zip);
        }

        // The catalogue as the archive gives it: each name's bytes, and the
        // digest of what the entry holds.
        let mut archive =
            zip::ZipArchive::new(fs::File::open(&zip).expect("archive")).expect("zip");
        let mut catalogue = Vec::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).expect("entry");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read");
            catalogue.push((
                entry.name_raw().to_vec(),
                entry.name().expect("name").to_string(),
                sha256_hex(&bytes),
            ));
        }
        assert_eq!(catalogue[0].0, nfd.as_bytes());
        // Since `zip` 9 (#807, #789) the crate reads a name as UTF-8 first,
        // whatever the flag says: both archives give `nfd`. Until 8.6 an
        // unflagged name came back as CP437, not what the bytes say.
        assert_eq!(catalogue[0].1, nfd, "flagged: {flagged}");

        export::build(
            &zip,
            dir.path(),
            "test source",
            support::fonts(),
            &aruna::job::Job::unattended(),
        )
        .expect("the package builds");
        let manifest =
            fs::read_to_string(dir.path().join(PACKAGE).join(export::MANIFEST)).expect("manifest");
        let groups = &manifest[manifest.find("\"groups\": [").expect("groups")..];
        let entries = values_of(groups, "source_entry");
        let digests = values_of(groups, "source_sha256");
        assert_eq!(entries.len(), 2, "{groups}");
        assert!(entries.iter().any(|e| e == nfd), "{entries:?}");
        assert!(!manifest.contains(nfc), "the name was normalised");
        for (entry, digest) in entries.iter().zip(&digests) {
            let (_, _, read) = catalogue
                .iter()
                .find(|(raw, _, _)| raw == entry.as_bytes())
                .unwrap_or_else(|| panic!("{entry:?} is not a name of the archive"));
            assert_eq!(read, digest, "{entry}");
        }
    }
}

/// Two builds of one archive give one manifest, byte for byte – the run
/// identifier among it: it names the archive and the program, not the hour.
#[test]
fn two_builds_give_one_manifest() {
    let dir = tempdir().expect("tempdir");
    let (_, root) = built(dir.path());
    let first = fs::read(root.join(export::MANIFEST)).expect("manifest");
    let other = dir.path().join("again");
    fs::create_dir(&other).expect("destination");
    export::build(
        &dir.path().join("corpus.zip"),
        &other,
        "test source",
        support::fonts(),
        &aruna::job::Job::unattended(),
    )
    .expect("builds again");
    assert_eq!(
        first,
        fs::read(other.join(PACKAGE).join(export::MANIFEST)).expect("manifest")
    );
}
