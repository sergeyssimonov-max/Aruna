//! The ceiling of the package, at its edge: every file of the package is put
//! to it before it is created, exactly the limit is room, and one byte more
//! stops the build with that file absent and the reader's package untouched.
//!
//! The limit is the argument of [`build_within`] and of [`PackageBytes`], so
//! the edge is a few kilobytes here and eight gibibytes nowhere.
use super::*;
use std::collections::BTreeMap;
use tempfile::tempdir;

/// A manuscript of the corpus's shape, as `fixtures/xml/valid/sections.xml`.
fn manuscript(siglum: &str, cth: &str, body: &str) -> String {
    format!(
        "<AOxml xmlns:AO=\"http://hethiter.net/ns/AO/1.0\"><AOHeader><docID>{siglum}</docID><CTHNr>{cth}</CTHNr><AO:InvNr>VAT 1</AO:InvNr><meta><uebern editor=\"AA\" date=\"2020-01-01\"/></meta></AOHeader><body><text><parlbk n=\"1\"/><l lg=\"Hit\"/>{body}</text></body></AOxml>"
    )
}

/// The entries of the archive, in the order the archive holds them – which is
/// the order their XML is written in, and not the order of display, in which
/// the PDFs are. The second is the document that gets no PDF: two repeated
/// clusters in a row.
const ENTRIES: [(&str, &str, &str, &str); 3] = [
    (
        "root/CTH 2_XML_HFR/KUB 1.1.xml",
        "KUB 1.1",
        "CTH 2",
        "a longer first section of another group",
    ),
    (
        "root/CTH 1_XML_HFR/KBo 3.6.xml",
        "KBo 3.6",
        "CTH 1",
        "\u{160}\u{303}\u{160}\u{303}",
    ),
    (
        "root/CTH 1_XML_HFR/KBo 3.5.xml",
        "KBo 3.5",
        "CTH 1",
        "first section",
    ),
];

fn source(dir: &Path) -> (PathBuf, crate::pdf::Fonts) {
    let path = dir.join("source.zip");
    let mut zip = zip::ZipWriter::new(File::create(&path).expect("archive"));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (entry, siglum, cth, body) in ENTRIES {
        zip.start_file(entry, options).expect("entry");
        zip.write_all(manuscript(siglum, cth, body).as_bytes())
            .expect("manuscript");
    }
    zip.finish().expect("finish ZIP");
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts");
    (
        path,
        crate::pdf::Fonts::load(&fonts).expect("project fonts"),
    )
}

/// Every file under `root`, by its path inside it, with its bytes.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, relative: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).expect("directory") {
            let entry = entry.expect("entry");
            let name = relative.join(entry.file_name());
            if entry.file_type().expect("file type").is_dir() {
                visit(root, &name, files);
            } else {
                files.insert(name, fs::read(entry.path()).expect("file bytes"));
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, Path::new(""), &mut files);
    files
}

/// The label every run here builds under: the inventory and the manifest
/// carry it, so their sizes are the same from run to run.
const LABEL: &str = "replacement";

/// One staging of the archive under `limit`, left on disk as it stopped.
struct Run {
    outcome: Result<()>,
    /// The XML of each entry in archive order, then the PDF of each document
    /// in display order: where [`stage`] is to write them, built or not.
    xml: Vec<PathBuf>,
    pdf: Vec<PathBuf>,
    files: BTreeMap<PathBuf, Vec<u8>>,
    /// Whether anything at all – a folder included – is in the staging.
    empty: bool,
}

fn stage_within(zip: &Path, fonts: &crate::pdf::Fonts, limit: u64) -> Run {
    let dir = tempdir().expect("staging parent");
    let staging = dir.path().join("staging");
    fs::create_dir(&staging).expect("staging");
    let job = Job::unattended();
    let Headers {
        mut archive,
        digest,
        mut fragments,
        not_manuscripts,
        ..
    } = read_headers(zip, &job, false).expect("headers");
    sort_by_display_order(&mut fragments, |f| &f.record);
    let documents = Document::place_all(fragments).expect("placed");
    let xml = ENTRIES
        .iter()
        .map(|(entry, ..)| {
            documents
                .iter()
                .find(|d| d.sums.source_entry == *entry)
                .expect("every entry is a document")
                .place
                .relative
                .clone()
        })
        .collect();
    let pdf = documents
        .iter()
        .map(|d| pdf_path(&d.place.relative))
        .collect();
    let source = manifest::Source {
        label: LABEL,
        archive_md5: &digest,
        not_manuscripts: &not_manuscripts,
    };
    let outcome = stage(
        &staging,
        &mut archive,
        documents,
        (&source, None),
        fonts,
        &job,
        &mut PackageBytes::within(limit),
    )
    .map(|_| ());
    Run {
        outcome,
        xml,
        pdf,
        files: files(&staging),
        empty: fs::read_dir(&staging).expect("staging").next().is_none(),
    }
}

/// The whole package under the production limit, and its files in the order
/// they are written, each with its size.
fn reference(zip: &Path, fonts: &crate::pdf::Fonts) -> (Run, Vec<(PathBuf, u64)>) {
    let whole = stage_within(zip, fonts, MAX_PACKAGE);
    whole.outcome.as_ref().expect("the package is staged");
    let root = [
        crate::paths::OUTPUT_FILE_NAME,
        crate::fonts::PACKAGED_FONT,
        crate::fonts::PACKAGED_TERMS,
        crate::cth_titles::PACKAGED_TERMS,
        MANIFEST,
    ];
    let order: Vec<(PathBuf, u64)> = whole
        .xml
        .iter()
        .chain(whole.pdf.iter().filter(|p| whole.files.contains_key(*p)))
        .cloned()
        .chain(root.iter().map(PathBuf::from))
        .map(|path| {
            let size = whole.files[&path].len() as u64;
            (path, size)
        })
        .collect();
    // The order names every file of the package, once.
    let named: std::collections::BTreeSet<&PathBuf> = order.iter().map(|(p, _)| p).collect();
    assert_eq!(named.len(), order.len());
    assert_eq!(named, whole.files.keys().collect());
    (whole, order)
}

fn refused(outcome: &Result<()>) -> (u64, u64) {
    match outcome {
        Err(ArunaError::ExportPackageTooLarge { written, limit }) => (*written, *limit),
        other => panic!("expected the package to be refused as too large, got {other:?}"),
    }
}

/// What a staging stopped before the file at `stop` must hold: the files
/// before it, with the bytes of the whole package, and nothing else.
fn assert_stopped_before(run: &Run, whole: &Run, order: &[(PathBuf, u64)], stop: usize) {
    let (name, _) = &order[stop];
    let wanted: BTreeMap<PathBuf, Vec<u8>> = order[..stop]
        .iter()
        .map(|(path, _)| (path.clone(), whole.files[path].clone()))
        .collect();
    assert!(
        !run.files.contains_key(name),
        "{} was created",
        name.display()
    );
    assert!(
        run.files == wanted,
        "stopped before {}: the staging holds {:?}, expected {:?}",
        name.display(),
        run.files.keys().collect::<Vec<_>>(),
        wanted.keys().collect::<Vec<_>>()
    );
}

#[test]
fn the_package_ceiling_admits_its_own_limit_and_refuses_one_byte_more() {
    assert!(within_package_ceiling(1024, 1024).is_ok());
    match within_package_ceiling(1025, 1024) {
        Err(ArunaError::ExportPackageTooLarge { written, limit }) => {
            assert_eq!((written, limit), (1025, 1024));
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_counter_admits_its_limit_and_does_not_write_one_byte_more() {
    let mut package = PackageBytes::within(10);
    package.write(4, || Ok(())).expect("four of ten");
    package.write(6, || Ok(())).expect("exactly the limit");
    assert_eq!(package.written, 10);
    package.write(0, || Ok(())).expect("nothing is room enough");

    let mut called = false;
    let outcome = package.write(1, || {
        called = true;
        Ok(())
    });
    assert_eq!(refused(&outcome), (11, 10));
    assert!(!called, "the write ran after the refusal");
    assert_eq!(package.written, 10, "a refused file was counted");
    let error = outcome.expect_err("the next byte does not fit");
    let failure = crate::app::Failure::of(&error);
    assert_eq!(failure.code, "package_too_large");
    assert_eq!(
        failure.message,
        "writing the next file would bring the package to 11 bytes, exceeding the limit of 10 bytes"
    );
}

#[test]
fn a_file_that_failed_to_be_written_is_not_counted_and_its_error_is_the_same() {
    let mut package = PackageBytes::within(10);
    let failure = package.write(4, || {
        Err(ArunaError::io("/nowhere")(std::io::Error::other(
            "disk failure",
        )))
    });
    assert!(
        matches!(&failure, Err(ArunaError::Io { path, source })
            if path == Path::new("/nowhere") && source.to_string() == "disk failure"),
        "{failure:?}"
    );
    assert_eq!(package.written, 0);
    package
        .write(10, || Ok(()))
        .expect("the room is still there");
}

#[test]
fn a_sum_past_the_largest_number_is_refused_and_not_wrapped() {
    let mut package = PackageBytes::within(u64::MAX);
    package.write(usize::MAX, || Ok(())).expect("fits");
    let before = package.written;
    let outcome = package.write(usize::MAX, || Ok(()));
    // On a 64-bit target the second sum passes `u64`; it must not wrap to a
    // small number that fits.
    if u64::try_from(usize::MAX).is_ok_and(|len| before.checked_add(len).is_none()) {
        assert_eq!(refused(&outcome), (u64::MAX, u64::MAX));
        assert_eq!(package.written, before);
    } else {
        outcome.expect("fits in u64");
    }
}

/// **Every file of the package, at its own edge.** For each file in the
/// order it is written – the three XML, the PDFs, the inventory, the font,
/// its terms, the terms of the CTH titles, the manifest – a limit that ends
/// exactly with it admits it, and a limit one byte short stops the build
/// naming the size it would have reached, with the file not created and
/// everything before it whole.
#[test]
fn each_file_is_admitted_at_the_limit_and_not_created_one_byte_short() {
    let dir = tempdir().expect("fixture");
    let (zip, fonts) = source(dir.path());
    let (whole, order) = reference(&zip, &fonts);

    // The fixture is what the comments say: three XML, a PDF for two of them,
    // five files at the root.
    assert_eq!(whole.xml.len(), 3);
    let built: Vec<&PathBuf> = whole
        .pdf
        .iter()
        .filter(|p| whole.files.contains_key(*p))
        .collect();
    assert_eq!(built.len(), 2, "{built:?}");
    assert!(!whole.files.contains_key(Path::new("CTH 1/KBo 3.6.pdf")));
    assert_eq!(order.len(), 3 + 2 + 5);
    assert!(order.iter().all(|(_, size)| *size > 0));

    let mut before = 0u64;
    for (k, (name, size)) in order.iter().enumerate() {
        let with = before + size;

        let short = stage_within(&zip, &fonts, with - 1);
        assert_eq!(
            refused(&short.outcome),
            (with, with - 1),
            "{}",
            name.display()
        );
        assert_stopped_before(&short, &whole, &order, k);

        let exact = stage_within(&zip, &fonts, with);
        match order.get(k + 1) {
            Some((_, next)) => {
                assert_eq!(
                    refused(&exact.outcome),
                    (with + next, with),
                    "after {}",
                    name.display()
                );
                assert_stopped_before(&exact, &whole, &order, k + 1);
            }
            None => {
                exact.outcome.as_ref().expect("exactly the limit is room");
                assert!(exact.files == whole.files);
            }
        }
        before = with;
    }

    // Nothing at all is room for nothing: not the first document, and not the
    // folder it would have been filed in.
    let nothing = stage_within(&zip, &fonts, 0);
    assert_eq!(refused(&nothing.outcome), (order[0].1, 0));
    assert!(nothing.empty, "a folder was made for a file that was not");
}

/// The limits the build is stopped at: one byte short of an XML, of a PDF and
/// of each file at the root, by the name of the file that does not fit.
fn one_byte_short_of(order: &[(PathBuf, u64)]) -> Vec<(PathBuf, u64)> {
    let mut end = 0u64;
    let ends: Vec<(PathBuf, u64)> = order
        .iter()
        .map(|(path, size)| {
            end += size;
            (path.clone(), end - 1)
        })
        .collect();
    let first = |wanted: &dyn Fn(&Path) -> bool| {
        ends.iter()
            .find(|(path, _)| wanted(path))
            .cloned()
            .expect("the package has such a file")
    };
    let mut limits = vec![
        // The second XML: the first is already in the staging.
        ends[1].clone(),
        first(&|p| p.extension().is_some_and(|e| e == "pdf")),
    ];
    for root in [
        crate::paths::OUTPUT_FILE_NAME,
        crate::fonts::PACKAGED_FONT,
        crate::fonts::PACKAGED_TERMS,
        crate::cth_titles::PACKAGED_TERMS,
        MANIFEST,
    ] {
        limits.push(first(&|p| p == Path::new(root)));
    }
    limits
}

/// **A build stopped at the ceiling costs the reader nothing.** Whichever
/// file did not fit – an XML, a PDF, the inventory, the font, the terms, the
/// manifest – the package that was there is the same bytes, the staging is
/// gone, and the next build that fits is published whole.
#[test]
fn a_build_refused_at_the_ceiling_keeps_the_package_and_the_next_one_succeeds() {
    let dir = tempdir().expect("fixture");
    let (zip, fonts) = source(dir.path());
    let (whole, order) = reference(&zip, &fonts);
    let total: u64 = order.iter().map(|(_, size)| size).sum();

    let out = dir.path().join("out");
    fs::create_dir(&out).expect("destination");
    let target = out.join(PACKAGE);
    let job = Job::unattended();
    build(&zip, &out, "previous", &fonts, &job).expect("valid previous package");
    let previous = files(&target);
    assert!(previous != whole.files, "the two labels are two packages");

    for (name, limit) in one_byte_short_of(&order) {
        let failure = build_within(&zip, &out, LABEL, &fonts, &job, limit).map(|_| ());
        assert_eq!(refused(&failure), (limit + 1, limit), "{}", name.display());
        assert!(
            files(&target) == previous,
            "the package changed when {} did not fit",
            name.display()
        );
        let left: Vec<_> = fs::read_dir(&out)
            .expect("destination")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(left, [std::ffi::OsString::from(PACKAGE)], "{left:?}");
    }

    let (built, _) =
        build_within(&zip, &out, LABEL, &fonts, &job, total).expect("exactly the limit");
    assert_eq!(
        (built.documents, built.pdfs),
        (
            3,
            PdfCount {
                built: 2,
                refused: 1
            }
        )
    );
    assert!(files(&target) == whole.files);
    assert_eq!(fs::read_dir(&out).expect("destination").count(), 1);
}

/// The same with no package there before: nothing is left under its name,
/// nothing beside it, and the build that fits is the first package.
#[test]
fn a_first_build_refused_at_the_ceiling_leaves_the_destination_empty() {
    let dir = tempdir().expect("fixture");
    let (zip, fonts) = source(dir.path());
    let (whole, order) = reference(&zip, &fonts);
    let total: u64 = order.iter().map(|(_, size)| size).sum();

    let out = dir.path().join("out");
    fs::create_dir(&out).expect("destination");
    let job = Job::unattended();
    for (name, limit) in one_byte_short_of(&order) {
        let failure = build_within(&zip, &out, LABEL, &fonts, &job, limit).map(|_| ());
        assert_eq!(refused(&failure), (limit + 1, limit), "{}", name.display());
        assert_eq!(
            fs::read_dir(&out).expect("destination").count(),
            0,
            "{}",
            name.display()
        );
    }

    build_within(&zip, &out, LABEL, &fonts, &job, total).expect("exactly the limit");
    assert!(files(&out.join(PACKAGE)) == whole.files);
    // And the limit of every build there is admits it as well.
    build(&zip, &out, LABEL, &fonts, &job).expect("the production limit");
    assert!(files(&out.join(PACKAGE)) == whole.files);
}
