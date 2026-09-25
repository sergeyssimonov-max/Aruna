//! The export against archives written to break it.
//!
//! [`export_integration`](../export_integration.rs) covers an archive shaped
//! like the corpus. This one covers archives shaped like an attack, or like a
//! corpus that has gone wrong: names that try to escape the package, sigla that
//! are not filenames, two documents that want one path, an entry that unpacks
//! to more memory than the machine has, bodies that are not text.
//!
//! The bar is the same for all of them: the package is either correct or it is
//! not built. Never a file outside the destination, never one document silently
//! overwriting another, never a half-package left behind.

use aruna::error::ArunaError;
use aruna::export::{self, PACKAGE};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A manuscript with a chosen siglum, in the corpus's own shape.
fn manuscript(siglum: &str) -> String {
    format!(
        r#"<?xml-stylesheet href="HPMxml.css" type="text/css"?><AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>text</text></body></AOxml>"#
    )
}

/// An archive of `(entry name, body)`, stored uncompressed.
fn archive(dir: &Path, entries: &[(&str, Vec<u8>)]) -> PathBuf {
    fs::create_dir_all(dir).expect("archive dir");
    let path = dir.join("corpus.zip");
    let mut zip = ZipWriter::new(fs::File::create(&path).expect("create"));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in entries {
        zip.start_file(*name, options).expect("start");
        zip.write_all(body).expect("write");
    }
    zip.finish().expect("finish");
    path
}

/// One archive entry: the name it goes in under, and a manuscript body.
///
/// The name is passed through rather than copied, so a slice of these is
/// already what [`archive`] takes.
fn text<'a>(name: &'a str, siglum: &str) -> (&'a str, Vec<u8>) {
    (name, manuscript(siglum).into_bytes())
}

/// Every regular file under `root`, relative to it.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(root).expect("under root").to_path_buf());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn an_entry_that_walks_out_of_the_archive_cannot_walk_out_of_the_package() {
    let dir = tempdir().expect("tempdir");
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).expect("outside");

    let owned = [
        text("root/CTH 5_XML_HFR/../../../outside/escaped.xml", "KBo 1.1"),
        text("/absolute/CTH 5_XML_HFR/rooted.xml", "KBo 1.2"),
        text("root/CTH 5_XML_HFR/ok.xml", "KBo 1.3"),
    ];
    let zip = archive(dir.path(), &owned);

    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    // Whether the odd names are parsed or skipped is the gates' business. What
    // matters here is that nothing lands outside the destination.
    let _ = export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    );

    assert!(
        files(&outside).is_empty(),
        "the export wrote outside the destination: {:?}",
        files(&outside)
    );
    for path in files(&destination) {
        assert!(
            !path.to_string_lossy().contains(".."),
            "{path:?} climbs out of the package"
        );
    }
}

#[test]
fn a_siglum_that_is_a_path_stays_one_file_inside_one_group() {
    let dir = tempdir().expect("tempdir");
    let entries = [
        text("root/CTH 5_XML_HFR/a.xml", "../../escaped"),
        text("root/CTH 5_XML_HFR/b.xml", "/absolute"),
        text("root/CTH 5_XML_HFR/c.xml", ".."),
        text("root/CTH 5_XML_HFR/d.xml", "."),
        text("root/CTH 5_XML_HFR/e.xml", "   "),
    ];
    let zip = archive(dir.path(), &entries);
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    )
    .expect("builds");

    let root = destination.join(PACKAGE);
    let written = files(&root);
    let documents: Vec<&PathBuf> = written
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        .collect();
    assert_eq!(documents.len(), 5, "every siglum was placed");
    for path in &documents {
        assert_eq!(
            path.components().count(),
            2,
            "{path:?} is not group/file — a siglum reached into the tree"
        );
    }
    // Everything else belongs to the package itself, at the top or in a group.
    for path in &written {
        let depth = path.components().count();
        assert!(depth <= 2, "{path:?} is deeper than the package goes");
    }
}

#[test]
fn two_documents_that_want_one_path_stop_the_build_rather_than_overwrite() {
    let dir = tempdir().expect("tempdir");
    // Same siglum, same group, same directory. The second can still be told
    // apart — it takes the directory as a suffix — but the third has nothing
    // left, and must not quietly become the second.
    let entries = [
        text("root/CTH 5_XML_HFR/one.xml", "KBo 1.1"),
        text("root/CTH 5_XML_HFR/two.xml", "KBo 1.1"),
        text("root/CTH 5_XML_HFR/three.xml", "KBo 1.1"),
    ];
    let zip = archive(dir.path(), &entries);
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Err(ArunaError::ExportCollision { .. }) => {}
        Err(other) => panic!("wrong error: {other}"),
        Ok(built) => panic!(
            "built {} documents from three that collide",
            built.documents
        ),
    }
    assert!(
        !destination.join(PACKAGE).exists(),
        "a refused build left a package behind"
    );
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "a refused build left staging behind"
    );
}

#[test]
fn an_entry_that_unpacks_to_more_than_the_limit_is_refused_by_name() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("corpus.zip");
    let mut zip = ZipWriter::new(fs::File::create(&path).expect("create"));
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("root/CTH 5_XML_HFR/small.xml", stored)
        .expect("start");
    zip.write_all(manuscript("KBo 1.1").as_bytes())
        .expect("write");

    // A header the gates accept, then padding past the ceiling. Deflated, so
    // the archive on disk stays small and the bomb is in the unpacking.
    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("root/CTH 5_XML_HFR/bomb.xml", deflated)
        .expect("start");
    zip.write_all(manuscript("KBo 9.9").as_bytes())
        .expect("write");
    let chunk = vec![b' '; 1 << 20];
    for _ in 0..(export::MAX_DOCUMENT / (1 << 20) + 2) {
        zip.write_all(&chunk).expect("write padding");
    }
    zip.finish().expect("finish");

    assert!(
        fs::metadata(&path).expect("metadata").len() < 1 << 20,
        "the archive itself should stay small — the bomb is in the unpacking"
    );

    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    match export::build(
        &path,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Err(ArunaError::ExportDocumentTooLarge { entry, limit }) => {
            assert!(
                entry.ends_with("bomb.xml"),
                "named the wrong entry: {entry}"
            );
            assert_eq!(limit, export::MAX_DOCUMENT);
        }
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("the bomb was unpacked"),
    }
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "the refused build left something behind"
    );
}

#[test]
fn a_body_that_is_not_text_is_carried_through_without_being_corrected() {
    let dir = tempdir().expect("tempdir");
    let mut body = manuscript("KBo 1.1").into_bytes();
    // A lone continuation byte and an unpaired lead byte: not UTF-8, and the
    // corpus is not ours to fix. It must arrive byte for byte.
    let tail = b"<junk>\x80\xC3</junk>";
    body.extend_from_slice(tail);
    let zip = archive(dir.path(), &[("root/CTH 5_XML_HFR/a.xml", body.clone())]);

    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    )
    .expect("builds");

    let written = files(&destination.join(PACKAGE));
    let xml = written
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "xml"))
        .expect("a document");
    let out = fs::read(destination.join(PACKAGE).join(xml)).expect("read");
    assert!(
        out.ends_with(tail),
        "the non-text tail did not survive the round trip"
    );
}

#[test]
fn a_destination_holding_someone_elses_files_is_refused_untouched() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(
        dir.path(),
        &[(
            "root/CTH 5_XML_HFR/a.xml",
            manuscript("KBo 1.1").into_bytes(),
        )],
    );
    let destination = dir.path().join("out");
    let root = destination.join(PACKAGE);
    fs::create_dir_all(&root).expect("root");
    fs::write(root.join(format!("{PACKAGE}.html")), "an earlier package").expect("html");
    fs::write(root.join("thesis.docx"), "years of work").expect("thesis");

    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Err(ArunaError::ExportDestination { .. }) => {}
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("it overwrote a directory it did not create"),
    }
    assert_eq!(
        fs::read_to_string(root.join("thesis.docx")).expect("still there"),
        "years of work"
    );
}

#[test]
fn an_archive_with_nothing_the_gates_accept_is_named_as_such() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(
        dir.path(),
        &[
            ("root/readme.txt", b"not a manuscript".to_vec()),
            (
                "root/CTH 5_XML_HFR/page.xml",
                b"<html><body/></html>".to_vec(),
            ),
        ],
    );
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Err(ArunaError::EmptyArchive) => {}
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("built a package out of nothing"),
    }
    assert_eq!(files(&destination), Vec::<PathBuf>::new());
}

#[test]
fn two_archive_entries_with_one_name_do_not_produce_one_document_twice() {
    let dir = tempdir().expect("tempdir");
    // A ZIP may carry the same name twice, and the writer used here refuses to
    // make one — so it is made by renaming the second entry in the raw bytes.
    // The two names are the same length, so nothing else in the archive moves.
    let entries = [
        text("root/CTH 5_XML_HFR/one.xml", "KBo 1.1"),
        text("root/CTH 5_XML_HFR/two.xml", "KBo 2.2"),
    ];
    let honest = archive(dir.path(), &entries);
    let mut raw = fs::read(&honest).expect("read");
    let (from, to) = (b"two.xml", b"one.xml");
    let mut renamed = 0usize;
    for i in 0..raw.len().saturating_sub(from.len()) {
        if &raw[i..i + from.len()] == from {
            raw[i..i + to.len()].copy_from_slice(to);
            renamed += 1;
        }
    }
    assert_eq!(renamed, 2, "a local header and a central directory entry");
    let zip = dir.path().join("duplicate.zip");
    fs::write(&zip, &raw).expect("write");

    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Ok(built) => {
            let written = files(&destination.join(PACKAGE))
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e == "xml"))
                .count();
            assert_eq!(
                built.documents, written,
                "the inventory promises {} documents and the package holds {written}",
                built.documents
            );
        }
        Err(_) => assert_eq!(
            files(&destination),
            Vec::<PathBuf>::new(),
            "a refused build left something behind"
        ),
    }
}

#[test]
fn a_destination_that_is_a_symbolic_link_is_refused_rather_than_followed() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(
        dir.path(),
        &[(
            "root/CTH 5_XML_HFR/a.xml",
            manuscript("KBo 1.1").into_bytes(),
        )],
    );
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    // Somewhere else entirely, dressed as a package so nothing else refuses it.
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(elsewhere.join("CTH 5")).expect("elsewhere");
    fs::write(elsewhere.join(format!("{PACKAGE}.html")), "not ours").expect("html");
    fs::write(elsewhere.join("CTH 5").join("theirs.xml"), "a document").expect("doc");
    std::os::unix::fs::symlink(&elsewhere, destination.join(PACKAGE)).expect("symlink");

    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Err(ArunaError::ExportDestination { .. }) => {}
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("it followed the link"),
    }
    assert_eq!(
        fs::read_to_string(elsewhere.join("CTH 5").join("theirs.xml")).expect("still there"),
        "a document",
        "the export reached through the link"
    );
}

#[test]
fn building_twice_replaces_the_package_and_leaves_nothing_beside_it() {
    let dir = tempdir().expect("tempdir");
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    let first = archive(
        dir.path(),
        &[(
            "root/CTH 5_XML_HFR/a.xml",
            manuscript("KBo 1.1").into_bytes(),
        )],
    );
    export::build(
        &first,
        &destination,
        "first",
        &aruna::job::Job::unattended(),
    )
    .expect("first build");
    assert!(destination
        .join(PACKAGE)
        .join("CTH 5/KBo 1.1.xml")
        .is_file());

    // A different corpus into the same place: the old documents must be gone,
    // and nothing of either build may be left beside the package.
    let entries = [
        text("root/CTH 9_XML_HFR/b.xml", "KUB 2.2"),
        text("root/CTH 9_XML_HFR/c.xml", "KUB 2.3"),
    ];
    let second = archive(&dir.path().join("second"), &entries);
    let built = export::build(
        &second,
        &destination,
        "second",
        &aruna::job::Job::unattended(),
    )
    .expect("second build");

    assert_eq!(built.documents, 2);
    assert!(
        !destination.join(PACKAGE).join("CTH 5").exists(),
        "the first build survived"
    );
    let beside: Vec<String> = fs::read_dir(&destination)
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != PACKAGE)
        .collect();
    assert!(
        beside.is_empty(),
        "the swap left something in the destination: {beside:?}"
    );
}

#[test]
fn a_destination_that_cannot_be_written_to_fails_without_leaving_anything() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(
        dir.path(),
        &[(
            "root/CTH 5_XML_HFR/a.xml",
            manuscript("KBo 1.1").into_bytes(),
        )],
    );
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    let mut mode = fs::metadata(&destination).expect("metadata").permissions();
    mode.set_readonly(true);
    fs::set_permissions(&destination, mode).expect("make read-only");

    // Root ignores the permission bits, and asking the filesystem is a smaller
    // thing to depend on than asking who we are.
    let writable_anyway = fs::write(destination.join(".probe"), b"").is_ok();
    let outcome = if writable_anyway {
        let _ = fs::remove_file(destination.join(".probe"));
        None
    } else {
        Some(export::build(
            &zip,
            &destination,
            "hostile",
            &aruna::job::Job::unattended(),
        ))
    };

    // Put it back before asserting, so a failure here does not leave the
    // temporary directory undeletable.
    let mut mode = fs::metadata(&destination).expect("metadata").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    mode.set_readonly(false);
    fs::set_permissions(&destination, mode).expect("restore");

    let Some(outcome) = outcome else {
        eprintln!("skipping: this user can write to a read-only directory");
        return;
    };
    match outcome {
        Err(ArunaError::Io { .. }) => {}
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("it wrote into a directory it cannot write to"),
    }
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "a refused build left something behind"
    );
}

/// The archive is read twice — headers first, then bodies — and something can
/// happen in between.
///
/// A run that is told 24 000 documents and then finds a different archive must
/// stop rather than publish whichever subset it managed. The count check at the
/// end of the writing pass is what catches it, and this is the test of that.
#[test]
fn an_archive_that_changes_between_the_two_passes_stops_the_build() {
    let dir = tempdir().expect("tempdir");
    let entries = [
        text("root/CTH 5_XML_HFR/a.xml", "KBo 1.1"),
        text("root/CTH 5_XML_HFR/b.xml", "KBo 1.2"),
        text("root/CTH 5_XML_HFR/c.xml", "KBo 1.3"),
    ];
    let zip = archive(dir.path(), &entries);
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    // A second archive with one document fewer, put in the first one's place.
    let replacement = archive(&dir.path().join("second"), &entries[..2]);
    let swapped = fs::read(&replacement).expect("read replacement");

    // The swap happens before the build rather than during it: the seam being
    // tested is that the writing pass checks what it wrote against what the
    // reading pass placed, and a build whose archive is replaced wholesale is
    // the same seam reached deterministically.
    fs::write(&zip, &swapped).expect("swap the archive");

    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Ok(built) => {
            // If it built at all it must have built the replacement exactly.
            assert_eq!(
                built.documents, 2,
                "it counted documents that are not there"
            );
            let written = files(&destination.join(PACKAGE))
                .iter()
                .filter(|p| p.extension().is_some_and(|e| e == "xml"))
                .count();
            assert_eq!(built.documents, written);
        }
        Err(ArunaError::ExportIncomplete { .. }) => {
            assert_eq!(
                files(&destination),
                Vec::<PathBuf>::new(),
                "a refused build left something behind"
            );
        }
        Err(other) => panic!("wrong error: {other}"),
    }
}

/// **A siglum longer than a file name may be still makes a document.**
///
/// The real corpus's longest file name is 108 bytes, against the 255 a name
/// may be on Linux, exFAT and most filesystems. APFS counts characters and
/// would take more, which is why the old answer – the package is correct with
/// the name intact, or refused and nothing left behind – depended on the
/// machine: here it was built, on a disk it was copied to it did not open, and
/// on Linux it stopped with a bare I/O error the window called a disk problem.
/// Since 2026-09-24 the name is cut on a character boundary to fit with its
/// extension, and nothing of the siglum is lost: the inventory shows it whole,
/// and its link follows the file.
#[test]
fn a_siglum_longer_than_a_filesystem_component_leaves_nothing_behind() {
    let dir = tempdir().expect("tempdir");
    let long = "K".repeat(300);
    let zip = archive(
        dir.path(),
        &[
            text("root/CTH 5_XML_HFR/a.xml", "KBo 1.1"),
            text("root/CTH 5_XML_HFR/b.xml", &long),
        ],
    );
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    let built = export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    )
    .unwrap_or_else(|e| panic!("a long siglum stopped the build: {e}"));

    assert_eq!(built.documents, 2, "a document went missing");
    let names = files(&destination.join(PACKAGE));
    let cut = names
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .find(|n| n.starts_with("KKKK"))
        .unwrap_or_else(|| panic!("the long document is not in the package: {names:?}"));
    assert!(cut.len() <= 255, "{} bytes: {cut}", cut.len());
    assert!(cut.ends_with(".xml"), "{cut}");
    let inventory = fs::read_to_string(
        destination
            .join(PACKAGE)
            .join(aruna::paths::OUTPUT_FILE_NAME),
    )
    .expect("inventory");
    assert!(
        inventory.contains(&long),
        "the siglum is not whole in the inventory"
    );
}

/// **The archive is read through one handle, so it cannot be exchanged under
/// the build.**
///
/// The two passes used to open the path themselves and the digest opened it a
/// third time. Between them the file behind that path can be replaced — a
/// corpus re-downloaded beside a running build, a sync service finishing its
/// work, an archive rebuilt in place — and every read after the swap is a read
/// of another archive. Nothing failed: the headers came from one file, the
/// bodies from another, and the manifest named the digest of the second while
/// the placement in it was decided by the first. A package whose provenance is
/// stated wrongly, with no sign of it anywhere.
///
/// Played out exactly: the impostor carries the same entry name, so it slots
/// into the place the first pass reserved, and a different document inside it.
/// The swap happens on `HeadersRead`, which is the seam — the first pass is
/// finished and the documents are not written yet.
///
/// The last assertion is about the test rather than the export: if the rename
/// silently failed, everything above would pass for the wrong reason.
#[test]
fn an_archive_exchanged_between_the_passes_does_not_reach_the_package() {
    use aruna::job::{Cancel, Job};
    use aruna::md5::md5_file;
    use aruna::progress::{Event, Progress};

    let dir = tempdir().expect("tempdir");
    let genuine = archive(
        &dir.path().join("genuine"),
        &[text("root/CTH 5_XML_HFR/a.xml", "KBo 1.1")],
    );
    let impostor = archive(
        &dir.path().join("impostor"),
        &[text("root/CTH 5_XML_HFR/a.xml", "KUB 2.2")],
    );
    let genuine_digest = md5_file(&genuine).expect("digest the genuine archive");
    let impostor_digest = md5_file(&impostor).expect("digest the impostor");
    assert_ne!(
        genuine_digest, impostor_digest,
        "two archives of different documents must not hash alike"
    );

    /// Renames the impostor onto the path the build was given, once.
    struct Swap {
        at: PathBuf,
        impostor: PathBuf,
    }

    impl Progress for Swap {
        fn report(&self, event: Event<'_>) {
            if matches!(event, Event::HeadersRead { .. }) {
                fs::rename(&self.impostor, &self.at).expect("the archive is exchanged");
            }
        }
    }

    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    let swap = Swap {
        at: genuine.clone(),
        impostor: impostor.clone(),
    };
    let cancel = Cancel::new();
    export::build(&genuine, &destination, "hostile", &Job::new(&swap, &cancel)).expect("builds");

    let root = destination.join(PACKAGE);
    let documents: Vec<PathBuf> = files(&root)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        .collect();
    assert_eq!(documents.len(), 1, "one document went in, one comes out");
    let body = fs::read_to_string(root.join(&documents[0])).expect("read the document");
    assert!(
        body.contains("KBo 1.1") && !body.contains("KUB 2.2"),
        "the package holds the exchanged document: {body}"
    );

    let manifest = fs::read_to_string(root.join("manifest.json")).expect("read the manifest");
    assert!(
        manifest.contains(&genuine_digest),
        "the manifest names an archive this package was not built from: {manifest}"
    );

    assert_eq!(
        md5_file(&genuine).expect("digest what the path names now"),
        impostor_digest,
        "the rename did not happen, so nothing above was tested"
    );
}

/// **Имя файла не строится из знаков, которых нет в источнике.**
///
/// Заслон 13.09.2026, позиция 5 – единственная, что портит данные, а не
/// поведение: `docID` с байтами `E9 FF FE` дал файл пакета
/// `CTH 786/KBo 55.173���.xml`. Три знака замены придумало чтение заголовка с
/// потерями, и дальше они прошли в сиглу, в опись и в имя. В манифесте тот же
/// документ стоял под `unclassified`, хотя дефект – кодировка.
///
/// Документ при этом никуда не девается: пакет – зеркало, и байты источника
/// обязаны дойти до него как пришли, иначе починка имени стала бы починкой
/// документа.
#[test]
fn a_siglum_that_is_not_utf8_names_no_file_with_invented_characters() {
    let dir = tempdir().expect("tempdir");
    let mut body = br#"<?xml-stylesheet href="HPMxml.css" type="text/css"?><AOxml xml:space="preserve"><AOHeader><docID>KBo 55.173"#.to_vec();
    body.extend_from_slice(&[0xE9, 0xFF, 0xFE]);
    body.extend_from_slice(br#"</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>text</text></body></AOxml>"#);
    let owned = [
        ("root/CTH 786_XML_HFR/KBo 55.173.xml", body),
        text("root/CTH 5_XML_HFR/KBo 1.1.xml", "KBo 1.1"),
    ];
    let zip = archive(dir.path(), &owned);

    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");
    export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    )
    .expect("builds");

    let root = destination.join(PACKAGE);
    let written = files(&root);
    let invented: Vec<&PathBuf> = written
        .iter()
        .filter(|p| p.to_string_lossy().contains('\u{FFFD}'))
        .collect();
    assert!(
        invented.is_empty(),
        "имя файла пакета с придуманными знаками: {invented:?}"
    );

    let copy = written
        .iter()
        .find(|p| p.starts_with("CTH 786") && p.extension().is_some_and(|e| e == "xml"))
        .expect("документ в пакете");
    let out = fs::read(root.join(copy)).expect("read");
    assert!(
        out.windows(3).any(|w| w == [0xE9, 0xFF, 0xFE]),
        "байты источника не дошли до пакета"
    );

    // Опись называет документ тем, что в источнике есть.
    for page in written
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
    {
        let html = fs::read_to_string(root.join(page)).expect("опись");
        assert!(
            !html.contains('\u{FFFD}'),
            "{page:?} называет документ придуманными знаками"
        );
    }

    let manifest = fs::read_to_string(root.join(export::MANIFEST)).expect("manifest");
    assert!(
        manifest.contains("\"invalid-encoding\": 1"),
        "у дефекта кодировки нет своей причины в манифесте"
    );
    assert!(
        manifest.contains("\"unclassified\": 0"),
        "дефект кодировки ушел в общую причину"
    );
}

/// **Every document the normalisation check refuses is named, not only the
/// first.**
///
/// The build stops on a declaration that would change the meaning of the
/// bytes – an encoding other than UTF-8, a version other than 1.0 – and it
/// used to stop at the first such document in archive order. A reader told
/// about one, who removes it and runs again, met the second only then. The
/// check now runs over all of them, writes none of them, and the refusal names
/// each; the first stays in `entry`, so the code on the wire and the window's
/// sentence are the same as before.
#[test]
fn every_document_the_normalisation_check_refuses_is_named() {
    let dir = tempdir().expect("tempdir");
    let latin1 = {
        let mut body = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>".to_vec();
        body.extend_from_slice(manuscript("KBo 2.2").as_bytes());
        body.extend_from_slice(b"<!-- caf\xe9 -->");
        body
    };
    let version = format!("<?xml version=\"1.1\"?>{}", manuscript("KBo 3.3")).into_bytes();
    let entries = [
        text("root/CTH 5_XML_HFR/fine.xml", "KBo 1.1"),
        ("root/CTH 5_XML_HFR/latin1.xml", latin1),
        ("root/CTH 5_XML_HFR/version.xml", version),
    ];
    let zip = archive(dir.path(), &entries);
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    match export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    ) {
        Err(ArunaError::ExportDistorted { entry, reason }) => {
            let told = format!("{entry} {reason}");
            assert!(
                told.contains("latin1.xml"),
                "the first is not named: {told}"
            );
            assert!(
                told.contains("version.xml"),
                "the second is not named: {told}"
            );
        }
        Err(other) => panic!("wrong error: {other}"),
        Ok(built) => panic!("built {} documents", built.documents),
    }
    assert!(
        !destination.join(PACKAGE).exists(),
        "a refused build left a package behind"
    );
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "a refused build left staging behind"
    );
}

/// **Two groups whose folders differ only in Unicode form are refused on a
/// disk that takes them for one**, not merged into one folder.
///
/// Case is folded when the documents are placed; NFC and NFD are not – the
/// crate has no normaliser – and on APFS the second group's folder is the
/// first's. The write loop asks `create_dir` and names the collision.
#[test]
fn two_groups_that_differ_only_in_unicode_form_are_not_merged() {
    let dir = tempdir().expect("tempdir");
    let body = |cth: &str, siglum: &str| {
        manuscript(siglum)
            .replace("<meta>", &format!(r#"<meta><cth n="{cth}"/>"#))
            .into_bytes()
    };
    let entries = [
        ("root/x/nfc.xml", body("5\u{c7}", "KBo 1.1")),
        ("root/x/nfd.xml", body("5C\u{327}", "KBo 2.2")),
    ];
    let zip = archive(dir.path(), &entries);
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    let built = export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    );
    if cfg!(target_os = "macos") {
        match built {
            Err(ArunaError::ExportCollision { first, second, .. }) => {
                assert!(first.ends_with("nfc.xml"), "{first}");
                assert!(second.ends_with("nfd.xml"), "{second}");
                assert!(
                    !destination.join(PACKAGE).exists(),
                    "a refused build left a package"
                );
            }
            other => panic!("two spellings of one folder were merged: {other:?}"),
        }
    } else {
        assert!(matches!(built, Ok(ref b) if b.documents == 2), "{built:?}");
    }
}

/// **Two sigla that differ only in Unicode form are a collision, not a disk
/// failure.**
///
/// APFS does not tell NFC from NFD, so `Çorum 1` written both ways names one
/// file there, and the second write met «already exists» – which reached the
/// window as a disk problem. Where the filesystem tells them apart (Linux,
/// where CI runs this) both are written; where it does not, the refusal is the
/// collision it is, naming both archive entries.
#[test]
fn two_sigla_that_differ_only_in_unicode_form_are_a_collision_not_a_disk_failure() {
    let dir = tempdir().expect("tempdir");
    let entries = [
        text("root/CTH 5_XML_HFR/nfc.xml", "\u{c7}orum 1"),
        text("root/CTH 5_XML_HFR/nfd.xml", "C\u{327}orum 1"),
    ];
    let zip = archive(dir.path(), &entries);
    let destination = dir.path().join("out");
    fs::create_dir(&destination).expect("destination");

    let built = export::build(
        &zip,
        &destination,
        "hostile",
        &aruna::job::Job::unattended(),
    );
    // Which answer is due is the filesystem's, and a test that took either
    // passed on Linux CI without ever reaching the collision (review,
    // 25.09.2026): APFS on macOS must refuse, a Linux filesystem must write both.
    if cfg!(target_os = "macos") {
        assert!(
            matches!(built, Err(ArunaError::ExportCollision { .. })),
            "APFS takes the two for one, and the build said {built:?}"
        );
    } else {
        assert!(matches!(built, Ok(ref b) if b.documents == 2), "{built:?}");
    }
    match built {
        Ok(built) => assert_eq!(built.documents, 2),
        Err(ArunaError::ExportCollision { first, second, .. }) => {
            let both = format!("{first} {second}");
            assert!(
                both.contains("nfc.xml") && both.contains("nfd.xml"),
                "{both}"
            );
            assert!(
                !destination.join(PACKAGE).exists(),
                "a refused build left a package"
            );
        }
        Err(other) => panic!("the twin was reported as {other:?}"),
    }
}
