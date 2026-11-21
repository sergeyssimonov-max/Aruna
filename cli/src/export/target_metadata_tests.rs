//! The publication checks run synchronously on the build's calling thread.
use super::*;
use crate::job::Cancel;
use crate::progress::Progress;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{self, Write as _};
use tempfile::tempdir;

thread_local! {
    // Deny one metadata read of one target. Renames and cleanup always use the real filesystem.
    static DENY: RefCell<Option<(PathBuf, usize)>> = const { RefCell::new(None) };
}

const DENIED: &str = "cannot read package metadata";

pub(super) fn read_target_metadata(target: &Path) -> io::Result<fs::Metadata> {
    let denied = DENY.with_borrow_mut(|armed| {
        if let Some((path, remaining)) = armed.as_mut() {
            if path == target {
                *remaining -= 1;
                if *remaining == 0 {
                    *armed = None;
                    return true;
                }
            }
        }
        false
    });
    if denied {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, DENIED))
    } else {
        fs::symlink_metadata(target)
    }
}

fn assert_denied(target: &Path, lookup: usize, build: impl FnOnce() -> Result<Built>) {
    assert!(DENY.replace(Some((target.to_owned(), lookup))).is_none());
    let result = build();
    assert!(
        DENY.take().is_none(),
        "target lookup {lookup} was never reached"
    );
    assert!(
        matches!(&result, Err(ArunaError::Io { path, source }) if path == target
            && source.kind() == io::ErrorKind::PermissionDenied && source.to_string() == DENIED),
        "{result:?}"
    );
}

fn source(dir: &Path) -> (PathBuf, crate::pdf::Fonts) {
    let path = dir.join("source.zip");
    let mut zip = zip::ZipWriter::new(File::create(&path).expect("archive"));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("root/CTH 1_XML_HFR/KBo 3.5.xml", options)
        .expect("entry");
    zip.write_all(include_bytes!("../../fixtures/xml/valid/sections.xml"))
        .expect("manuscript");
    zip.finish().expect("finish ZIP");
    let fonts = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts");
    (
        path,
        crate::pdf::Fonts::load(&fonts).expect("project fonts"),
    )
}

fn package_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, relative: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).expect("package directory") {
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

fn check_existing_package(lookup: usize) {
    let dir = tempdir().expect("fixture");
    let (zip, fonts) = source(dir.path());
    let out = dir.path().join("out");
    fs::create_dir(&out).expect("destination");
    let target = out.join(PACKAGE);
    let job = Job::unattended();
    build(&zip, &out, "previous", &fonts, &job).expect("valid previous package");
    let previous = package_files(&target);

    assert_denied(&target, lookup, || {
        build(&zip, &out, "replacement", &fonts, &job)
    });

    assert_eq!(package_files(&target), previous);
    assert_eq!(fs::read_dir(&out).expect("destination").count(), 1);
}

#[test]
fn metadata_denied_on_the_initial_check_preserves_the_package() {
    check_existing_package(1);
}

#[test]
fn metadata_denied_on_the_locked_check_preserves_the_package() {
    check_existing_package(2);
}

#[test]
fn metadata_denied_when_moving_aside_preserves_the_package() {
    check_existing_package(3);
}

#[test]
fn metadata_denied_on_the_locked_check_preserves_an_empty_foreign_directory() {
    struct CreateEmpty<'a>(&'a Path);
    impl Progress for CreateEmpty<'_> {
        fn report(&self, event: Event<'_>) {
            if matches!(event, Event::CheckingPackage) {
                fs::create_dir(self.0).expect("empty directory after the initial check");
            }
        }
    }

    let dir = tempdir().expect("fixture");
    let (zip, fonts) = source(dir.path());
    let out = dir.path().join("out");
    fs::create_dir(&out).expect("destination");
    let target = out.join(PACKAGE);
    let progress = CreateEmpty(&target);
    let cancel = Cancel::new();
    assert_denied(&target, 2, || {
        build(
            &zip,
            &out,
            "candidate",
            &fonts,
            &Job::new(&progress, &cancel),
        )
    });
    assert_eq!(
        fs::read_dir(&target)
            .expect("original empty directory")
            .count(),
        0
    );
    assert_eq!(fs::read_dir(&out).expect("destination").count(), 1);
}

#[test]
fn metadata_errors_other_than_not_found_keep_the_native_code_and_target_path() {
    let dir = tempdir().expect("fixture");
    let parent = dir.path().join("file");
    fs::write(&parent, b"original").expect("regular file");
    let target = parent.join(PACKAGE);
    let original = fs::symlink_metadata(&target).expect_err("parent is not a directory");
    assert_eq!(original.kind(), io::ErrorKind::NotADirectory);

    for result in [
        validate::check_destination(&target),
        Replaced::aside(&target, &parent).map(drop),
    ] {
        assert!(matches!(result, Err(ArunaError::Io { path, source })
            if path == target && source.kind() == original.kind() && source.raw_os_error() == original.raw_os_error()));
    }
    assert_eq!(fs::read(parent).expect("original parent"), b"original");
}
