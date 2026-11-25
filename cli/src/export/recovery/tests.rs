//! Pause a real child at the two rename boundaries. No signal is sent until
//! it reports that it holds the publication lock and reached the requested point.
use super::*;
use crate::export::{
    staging::{staging_name, Replaced, Staging},
    PACKAGE,
};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

fn build(dir: &Path, label: &str) {
    fs::create_dir_all(dir).unwrap();
    let zip = dir.join("fixture.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&zip).unwrap());
    writer
        .start_file(
            "root/CTH 1_XML_HFR/KBo 1.1.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    let xml =
        include_str!("../../../fixtures/xml/valid/sections.xml").replace("first section", label);
    writer.write_all(xml.as_bytes()).unwrap();
    writer.finish().unwrap();
    let fonts =
        crate::pdf::Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts"))
            .unwrap();
    crate::export::build(&zip, dir, label, &fonts, &Job::unattended()).unwrap();
}

fn publisher(out: &Path, phase: &str) {
    let pause = || {
        fs::File::create_new(out.join("reached-rename")).unwrap();
        let mut go = [0; 1];
        std::io::stdin().read_exact(&mut go).unwrap();
    };
    let lock = Publication::acquire(out, &Job::unattended()).unwrap();
    let target = out.join(PACKAGE);
    let staging = Staging::fresh(out.join(staging_name())).unwrap();
    fs::rename(out.join("new").join(PACKAGE), staging.path()).unwrap();
    let previous = Replaced::aside(&target, out).unwrap();
    if phase != "after" {
        pause();
    }
    previous
        .publish(staging, out, lock.exclusive, || {
            if phase == "after" {
                pause();
            }
            verify_copy(&target, PACKAGE)
        })
        .unwrap();
}

#[test]
fn reader_at_publication_boundaries() {
    if let Some(out) = std::env::var_os("ARUNA_TEST_RECOVERY_CHILD") {
        return publisher(
            Path::new(&out),
            &std::env::var("ARUNA_TEST_RECOVERY_PHASE").unwrap(),
        );
    }
    for (phase, occupied) in [
        ("before", false),
        ("before", true),
        ("after", false),
        ("concurrent", false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path();
        let target = out.join(PACKAGE);
        let previous = out.join(format!(".{PACKAGE}.previous"));
        build(out, "working revision");
        let working = snapshot(&target);
        build(&out.join("new"), "new revision");
        let new = snapshot(&out.join("new").join(PACKAGE));
        assert_ne!(new, working);
        let earlier = occupied.then(|| {
            build(&out.join("older"), "older revision");
            fs::rename(out.join("older").join(PACKAGE), &previous).unwrap();
            snapshot(&previous)
        });
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "export::recovery::tests::reader_at_publication_boundaries",
                "--nocapture",
            ])
            .env("ARUNA_TEST_RECOVERY_CHILD", out)
            .env("ARUNA_TEST_RECOVERY_PHASE", phase)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while !out.join("reached-rename").is_file() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "publisher exited before rename"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "publisher did not reach rename"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        if phase == "concurrent" {
            struct Waiting(mpsc::Sender<()>);
            impl crate::progress::Progress for Waiting {
                fn report(&self, event: crate::progress::Event<'_>) {
                    if matches!(event, crate::progress::Event::WaitingForPublication) {
                        self.0.send(()).unwrap();
                    }
                }
            }
            std::thread::scope(|scope| {
                let (tx, rx) = mpsc::channel();
                let reader = scope.spawn(move || {
                    reader_paths_with(out, &Job::new(&Waiting(tx), &crate::job::Cancel::new()))
                });
                let waiting = rx.recv_timeout(Duration::from_secs(10));
                let target_was_absent = !target.exists();
                child.stdin.take().unwrap().write_all(b"x").unwrap();
                assert!(child.wait().unwrap().success());
                waiting.unwrap();
                assert!(target_was_absent);
                assert_eq!(reader.join().unwrap().unwrap().0, target);
            });
        } else {
            child.kill().unwrap();
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(child.wait().unwrap().signal(), Some(9));
        }
        let (root, inventory) = reader_paths(out).unwrap();
        assert_eq!(root, target);
        assert!(inventory.is_file());
        assert_eq!(
            snapshot(&root),
            if phase == "before" { working } else { new }
        );
        if let Some(earlier) = earlier {
            assert_eq!(snapshot(&previous), earlier);
        }
        assert_eq!(reader_paths(out).unwrap(), (root, inventory));
    }
}

#[test]
fn conflicting_or_damaged_copies_remain_available_for_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path();
    let target = out.join(PACKAGE);
    let previous = out.join(format!(".{PACKAGE}.previous"));
    build(out, "before");
    fs::rename(&target, &previous).unwrap();
    let slot = out.join(format!("{}.rollback", staging_name()));
    fs::create_dir(&slot).unwrap();
    fs::write(slot.join(ROLLBACK_MARKER), b"").unwrap();
    build(&slot, "other");
    let backup = snapshot(&previous);
    let other = snapshot(&slot.join(PACKAGE));
    let error = reader_paths(out).unwrap_err().to_string();
    assert!(error.contains(previous.to_str().unwrap()) && error.contains(slot.to_str().unwrap()));
    assert!(!target.exists());
    assert_eq!(snapshot(&previous), backup);
    assert_eq!(snapshot(&slot.join(PACKAGE)), other);
    fs::rename(slot.join(PACKAGE), &target).unwrap();
    let xml = other
        .keys()
        .find(|p| p.extension().is_some_and(|s| s == "xml"))
        .unwrap();
    fs::write(target.join(xml), b"damaged").unwrap();
    assert!(reader_paths(out).is_err());
    assert_eq!(snapshot(&previous), backup);
    assert_eq!(fs::read(target.join(xml)).unwrap(), b"damaged");
}

#[test]
fn restore_never_replaces_an_empty_directory_created_after_inspection() {
    let dir = tempfile::tempdir().unwrap();
    let saved = dir.path().join(".TLHdig.previous");
    let working = dir.path().join(PACKAGE);
    fs::create_dir(&saved).unwrap();
    fs::write(saved.join("retained"), b"working copy").unwrap();
    fs::create_dir(&working).unwrap();
    assert!(restore_name(&saved, &working).is_err());
    assert_eq!(fs::read(saved.join("retained")).unwrap(), b"working copy");
    assert_eq!(fs::read_dir(working).unwrap().count(), 0);
}

#[test]
fn a_recovery_slot_with_two_package_names_is_left_intact() {
    let dir = tempfile::tempdir().unwrap();
    let slot = dir.path().join(format!("{}.rollback", staging_name()));
    build(&slot, "before");
    fs::write(slot.join(ROLLBACK_MARKER), b"").unwrap();
    fs::create_dir(slot.join(crate::export::LEGACY_PACKAGE)).unwrap();
    let before = snapshot(&slot);
    let error = reader_paths(dir.path()).unwrap_err().to_string();
    assert!(error.contains(slot.to_str().unwrap()));
    assert!(!dir.path().join(PACKAGE).exists());
    assert!(slot.join(crate::export::LEGACY_PACKAGE).is_dir());
    assert_eq!(snapshot(&slot), before);
}

#[test]
fn a_link_or_unverified_lock_cannot_authorize_a_restore() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join(PACKAGE);
    let saved = dir.path().join(format!(".{PACKAGE}.previous"));
    build(dir.path(), "readable");
    fs::rename(&target, &saved).unwrap();
    let before = snapshot(&saved);
    assert!(recover(&target, PACKAGE, std::slice::from_ref(&saved), false).is_err());
    assert!(!target.exists());
    fs::rename(&saved, dir.path().join("other")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("other"), &saved).unwrap();
    assert!(reader_paths(dir.path()).is_err());
    assert!(!target.exists());
    assert_eq!(snapshot(&dir.path().join("other")), before);
}

#[test]
fn truncated_pdf_is_reported_without_publishing_the_copy() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join(PACKAGE);
    let saved = dir.path().join(format!(".{PACKAGE}.previous"));
    build(dir.path(), "readable");
    fs::rename(&target, &saved).unwrap();
    let pdf = snapshot(&saved)
        .into_keys()
        .find(|p| p.extension().is_some_and(|s| s == "pdf"))
        .unwrap();
    fs::write(saved.join(&pdf), b"%PDF-1.7 truncated").unwrap();
    assert!(reader_paths(dir.path())
        .unwrap_err()
        .to_string()
        .contains("PDF"));
    assert_eq!(fs::read(saved.join(pdf)).unwrap(), b"%PDF-1.7 truncated");
    assert!(!target.exists());
}

#[test]
fn a_legacy_local_copy_uses_its_working_name_while_still_hidden() {
    let dir = tempfile::tempdir().unwrap();
    build(dir.path(), "legacy");
    let legacy = crate::export::LEGACY_PACKAGE;
    let saved = dir.path().join(format!(".{legacy}.previous"));
    fs::rename(dir.path().join(PACKAGE), &saved).unwrap();
    fs::rename(
        saved.join(format!("{PACKAGE}.html")),
        saved.join(format!("{legacy}.html")),
    )
    .unwrap();
    let manifest = saved.join(MANIFEST);
    let json = fs::read_to_string(&manifest)
        .unwrap()
        .replace(&format!("\"{PACKAGE}\""), &format!("\"{legacy}\""))
        .replace(
            &format!("\"{PACKAGE}.html\""),
            &format!("\"{legacy}.html\""),
        );
    fs::write(manifest, json).unwrap();
    let (root, inventory) = reader_paths(dir.path()).unwrap();
    assert_eq!(root, dir.path().join(legacy));
    assert!(inventory.is_file());
}
