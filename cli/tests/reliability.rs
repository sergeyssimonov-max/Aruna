//! Properties the next stage will depend on: the same input gives the same
//! output, and running the thing repeatedly does not accumulate anything.
//!
//! Both are cheap to check now and expensive to retrofit. A converter that maps
//! 23 936 documents to 23 936 PDFs has to put each one where the last run put
//! it — otherwise every re-run rewrites the whole corpus and nothing
//! incremental is possible — and a batch process that leaks a file descriptor
//! per document runs out of them at about the thousandth.

use aruna::export::{self, PACKAGE};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A manuscript in the corpus's own shape.
fn manuscript(siglum: &str) -> String {
    format!(
        r#"<?xml-stylesheet href="HPMxml.css" type="text/css"?><AOxml xml:space="preserve"><AOHeader><docID>{siglum}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>text</text></body></AOxml>"#
    )
}

/// An archive with the awkward shapes the corpus really has: one group filed
/// under two folders and not adjacent to itself, a siglum that repeats, and a
/// siglum with a slash in it.
fn archive(dir: &Path) -> PathBuf {
    let path = dir.join("corpus.zip");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).expect("create"));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, siglum) in [
        ("root/CTH 5_XML_HFR/KBo 1.1.xml", "KBo 1.1"),
        ("root/CTH 9_XML_HFR/KUB 2.1.xml", "KUB 2.1"),
        ("root/CTH 5_XML_TLH/KBo 1.1.xml", "KBo 1.1"),
        ("root/CTH 5_XML_HFR/544-f.xml", "544/f"),
        ("root/CTH 9_XML_TLH/KUB 2.2.xml", "KUB 2.2"),
    ] {
        zip.start_file(name, options).expect("start");
        zip.write_all(manuscript(siglum).as_bytes()).expect("write");
    }
    zip.finish().expect("finish");
    path
}

/// Every file under `root`, relative, with its contents.
fn contents(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path.strip_prefix(root).expect("under root").to_path_buf();
                out.insert(relative, std::fs::read(&path).expect("read"));
            }
        }
    }
    out
}

#[test]
fn two_builds_of_one_archive_are_byte_identical() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(dir.path());

    let mut built = Vec::new();
    for name in ["first", "second"] {
        let destination = dir.path().join(name);
        std::fs::create_dir(&destination).expect("destination");
        export::build(
            &zip,
            &destination,
            "reproducibility",
            &aruna::job::Job::unattended(),
        )
        .expect("builds");
        built.push(contents(&destination.join(PACKAGE)));
    }

    let (first, second) = (&built[0], &built[1]);
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        second.keys().collect::<Vec<_>>(),
        "the two builds hold different files"
    );
    for (path, bytes) in first {
        assert_eq!(
            bytes,
            &second[path],
            "{} differs between two builds of the same archive",
            path.display()
        );
    }
    assert!(first.len() > 5, "nothing was built, so nothing was proven");
}

/// Whether the source label reaches the output, and nothing else changes with
/// it. The label is the one thing a caller varies between runs.
#[test]
fn only_the_source_label_changes_when_the_source_label_changes() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(dir.path());

    let mut built = Vec::new();
    for (name, label) in [("a", "label one"), ("b", "label two")] {
        let destination = dir.path().join(name);
        std::fs::create_dir(&destination).expect("destination");
        export::build(&zip, &destination, label, &aruna::job::Job::unattended()).expect("builds");
        built.push(contents(&destination.join(PACKAGE)));
    }

    let differing: Vec<_> = built[0]
        .iter()
        .filter(|(path, bytes)| built[1][*path] != **bytes)
        .map(|(path, _)| path.clone())
        .collect();
    let expected: Vec<PathBuf> = vec![
        PathBuf::from(format!("{PACKAGE}.html")),
        PathBuf::from("manifest.json"),
    ];
    let mut sorted = differing.clone();
    sorted.sort();
    assert_eq!(
        sorted, expected,
        "changing the label changed something other than the two documents that record it"
    );
}

/// How many file descriptors this process holds open.
///
/// `/dev/fd` lists them on macOS and Linux alike. Reading the directory opens
/// one itself, which is why the number is only ever compared with another
/// reading taken the same way.
fn open_descriptors() -> usize {
    std::fs::read_dir("/dev/fd")
        .expect("/dev/fd")
        .flatten()
        .count()
}

#[test]
fn building_repeatedly_does_not_accumulate_file_descriptors() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(dir.path());
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    // One build first: the first run of anything opens things it then keeps.
    export::build(
        &zip,
        &destination,
        "warm-up",
        &aruna::job::Job::unattended(),
    )
    .expect("builds");
    let before = open_descriptors();

    for _ in 0..12 {
        export::build(
            &zip,
            &destination,
            "leak check",
            &aruna::job::Job::unattended(),
        )
        .expect("builds");
    }
    let after = open_descriptors();

    assert!(
        after <= before,
        "twelve builds left {} extra descriptor(s) open ({before} before, {after} after)",
        after.saturating_sub(before)
    );
}

#[test]
fn a_build_leaves_nothing_of_its_own_beside_the_package() {
    let dir = tempdir().expect("tempdir");
    let zip = archive(dir.path());
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    for _ in 0..3 {
        export::build(&zip, &destination, "repeat", &aruna::job::Job::unattended())
            .expect("builds");
        let beside: Vec<String> = std::fs::read_dir(&destination)
            .expect("read")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != PACKAGE)
            .collect();
        assert!(
            beside.is_empty(),
            "the build left {beside:?} in the destination"
        );
    }
}
