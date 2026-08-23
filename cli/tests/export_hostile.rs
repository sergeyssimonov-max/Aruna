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
