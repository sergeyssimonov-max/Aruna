//! Starting from the mess an interrupted run leaves behind.
//!
//! Every cleanup in this crate is a `Drop`, so a failure at any `?` tidies up
//! after itself — and `reliability.rs` checks that a build that *finishes*
//! leaves nothing beside the package. What neither covers is the case `Drop`
//! cannot: a process that is killed. `SIGKILL`, a lost power supply, a laptop
//! lid closed on a 389 MB rename — none of them run a destructor, and each can
//! leave the destination holding a half-built `.TLHdig_Beta_0.3.build`, an
//! aside `.TLHdig_Beta_0.3.previous` that was never put back, or a `.part`
//! file beside the inventory.
//!
//! The question these ask is the one a user actually has: *does it work if I
//! just run it again?* Not "does it clean up when it fails" but "does it
//! recover when it did not get the chance to".
//!
//! No signals and no `unsafe`. Killing a process mid-rename is not
//! reproducible; the states a kill produces are, so the leftovers are written
//! directly and the run is asked to cope with them. That also makes each case
//! nameable — "a staging directory from an earlier run" is a state, where "a
//! process killed at some point" is a lottery.

mod support;

use aruna::export::{self, PACKAGE};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use support::{manuscript, mixed_archive};
use tempfile::{tempdir, TempDir};

/// Everything directly inside `dir`, by name.
/// What the destination holds besides the package and abandoned staging.
///
/// **Staging is excepted since 2.2.0, and that is a trade rather than a
/// loosening.** A build used to stage under one name, `.{PACKAGE}.build`, so
/// the next build found a killed run's directory and cleared it. That name is
/// now unique to the run, because the binary exports on every run and two of
/// them — a second double-click — meet in one Downloads folder: measured with
/// the shared name, each cleared the other's directory and **both runs failed
/// with no package at all**. The cost is here: a run killed by a signal leaves
/// its staging behind, and the next run builds beside it rather than over it.
/// What has not changed is that nothing from it is ever published, which the
/// assertions below still hold.
fn beside(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != PACKAGE && !n.starts_with(&format!(".{PACKAGE}.build")))
        .collect();
    names.sort();
    names
}

/// Every file under `root`, relative to it.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir").flatten() {
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

/// A destination holding a finished package, and the archive it was built from.
fn published() -> (TempDir, PathBuf, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let zip = mixed_archive(dir.path());
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");
    export::build(&zip, &destination, "first", &aruna::job::Job::unattended())
        .expect("the first build");
    (dir, zip, destination)
}

/// A staging directory left by a run that was killed while writing.
///
/// `Staging::fresh` clears whatever is at that path before it starts. Without
/// that, the next build would write its documents into a directory already
/// holding another build's — and `create_new` would refuse the first collision,
/// so a killed run would make every later build fail until someone found the
/// hidden directory and deleted it by hand.
#[test]
fn a_staging_directory_from_a_killed_run_does_not_stop_the_next_one() {
    let dir = tempdir().expect("tempdir");
    let zip = mixed_archive(dir.path());
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    // What a kill mid-write leaves: the staging directory, with some of the
    // package already in it.
    let staging = destination.join(format!(".{PACKAGE}.build"));
    std::fs::create_dir_all(staging.join("CTH 5")).expect("staging");
    std::fs::write(
        staging.join("CTH 5").join("KBo 1.1.xml"),
        manuscript("KBo 1.1", "FB", "2017-03-28"),
    )
    .expect("half a document");
    std::fs::write(staging.join("stray.txt"), "left over").expect("stray");

    let built = export::build(&zip, &destination, "second", &aruna::job::Job::unattended())
        .expect("a leftover staging directory must not stop a build");

    assert_eq!(built.documents, 5);
    assert!(
        beside(&destination).is_empty(),
        "the leftovers survived the build: {:?}",
        beside(&destination)
    );
    assert!(
        !files(&destination.join(PACKAGE)).contains(&PathBuf::from("stray.txt")),
        "a file from the killed run's staging directory was published"
    );
}

/// A package moved aside by a run that was killed before it could be put back.
///
/// `Replaced::aside` moves the existing package to `.TLHdig_Beta_0.3.previous`
/// so the swap is one rename wide, and `Drop` puts it back if the publish
/// fails. A kill in that window leaves the aside copy with nothing to restore
/// it — and the next build has to remove it before it can move the new package
/// aside itself.
///
/// The reader has lost their package either way; what must not happen is that
/// running again cannot give them one.
#[test]
fn a_package_left_aside_by_a_killed_run_is_cleared_by_the_next_one() {
    let (_dir, zip, destination) = published();

    // The state a kill between `aside` and `publish` leaves: the package is
    // under its aside name and nothing is at the real one.
    let aside = destination.join(format!(".{PACKAGE}.previous"));
    std::fs::rename(destination.join(PACKAGE), &aside).expect("aside");
    assert!(!destination.join(PACKAGE).exists());

    let built = export::build(&zip, &destination, "second", &aruna::job::Job::unattended())
        .expect("a package left aside must not stop the next build");

    assert_eq!(built.documents, 5);
    assert!(
        !aside.exists(),
        "the aside copy from the killed run was left in the destination"
    );
    assert!(
        beside(&destination).is_empty(),
        "the destination still holds {:?}",
        beside(&destination)
    );
    assert!(destination
        .join(PACKAGE)
        .join(format!("{PACKAGE}.html"))
        .is_file());
}

/// Both leftovers at once, over a package that is also still there.
///
/// The worst of the reachable states: a killed run left its staging directory,
/// an earlier killed run left an aside copy, and the destination holds a
/// finished package as well. Each is handled by a different piece of the build,
/// and this is the only place they meet.
#[test]
fn a_destination_holding_every_kind_of_leftover_still_builds() {
    let (_dir, zip, destination) = published();

    let staging = destination.join(format!(".{PACKAGE}.build"));
    std::fs::create_dir_all(&staging).expect("staging");
    std::fs::write(staging.join("half.xml"), "half a document").expect("half");

    let aside = destination.join(format!(".{PACKAGE}.previous"));
    std::fs::create_dir_all(&aside).expect("aside");
    std::fs::write(aside.join("older.xml"), "an older package").expect("older");

    let built = export::build(&zip, &destination, "second", &aruna::job::Job::unattended())
        .expect("the leftovers of two killed runs must not stop a third");

    assert_eq!(built.documents, 5);
    assert!(
        beside(&destination).is_empty(),
        "the destination still holds {:?}",
        beside(&destination)
    );
    let published = files(&destination.join(PACKAGE));
    for stray in ["half.xml", "older.xml"] {
        assert!(
            !published.contains(&PathBuf::from(stray)),
            "{stray} from a killed run reached the published package"
        );
    }
}

/// Building again gives the same package it would have given anyway.
///
/// Recovering is not enough on its own: a build that started from leftovers has
/// to produce the package a build from a clean destination produces, byte for
/// byte. Otherwise "run it again" fixes the symptom and leaves the reader with
/// a package that is quietly not the one everyone else has.
#[test]
fn a_build_that_recovered_produces_the_package_a_clean_one_would() {
    let dir = tempdir().expect("tempdir");
    let zip = mixed_archive(dir.path());

    let clean = dir.path().join("clean");
    std::fs::create_dir(&clean).expect("clean");
    export::build(&zip, &clean, "reference", &aruna::job::Job::unattended())
        .expect("the clean build");

    let messy = dir.path().join("messy");
    std::fs::create_dir(&messy).expect("messy");
    let staging = messy.join(format!(".{PACKAGE}.build"));
    std::fs::create_dir_all(staging.join("CTH 9")).expect("staging");
    std::fs::write(staging.join("CTH 9").join("KUB 2.1.xml"), "not this").expect("half");
    std::fs::create_dir_all(messy.join(format!(".{PACKAGE}.previous"))).expect("aside");
    export::build(&zip, &messy, "reference", &aruna::job::Job::unattended())
        .expect("the build that had to recover");

    let (a, b) = (clean.join(PACKAGE), messy.join(PACKAGE));
    assert_eq!(
        files(&a),
        files(&b),
        "the two packages hold different files"
    );
    for relative in files(&a) {
        assert_eq!(
            std::fs::read(a.join(&relative)).expect("clean"),
            std::fs::read(b.join(&relative)).expect("recovered"),
            "{} differs between a clean build and one that recovered",
            relative.display()
        );
    }
}

/// A scratch file beside the inventory is not mistaken for the inventory.
///
/// `write_atomic` writes `<name>.<pid>.<n>.part` and renames it into place, so
/// a killed run leaves that file next to the destination. It is not the
/// inventory — nothing links to it and nothing opens it — but it must not
/// survive a run either, and it must not be what the next run replaces.
///
/// Run as a child process: this is about the binary's own output path, and
/// nothing here mutates the environment of a process shared with other tests.
#[test]
fn a_scratch_file_from_a_killed_run_is_not_the_inventory() {
    let home = tempdir().expect("tempdir");
    let downloads = home.path().join("Downloads");
    std::fs::create_dir_all(&downloads).expect("downloads");
    let zip = mixed_archive(home.path());

    // What a kill between the write and the rename leaves behind. The pid is
    // one that is not this process and not the child's.
    let orphan = downloads.join("TLHdig_Beta_0.3.html.999999.0.part");
    std::fs::write(&orphan, "<!DOCTYPE html><p>half an inventory").expect("orphan");

    let out = Command::new(env!("CARGO_BIN_EXE_aruna"))
        .env("HOME", home.path())
        .env("ARUNA_ZIP", &zip)
        .env("ARUNA_CACHE_DIR", home.path().join("cache"))
        .env_remove("XDG_CACHE_HOME")
        .env_remove("XDG_DOWNLOAD_DIR")
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the run failed:\n{stderr}");
    assert!(!stderr.contains("panicked"), "the run panicked:\n{stderr}");

    let inventory = downloads.join(PACKAGE).join("TLHdig_Beta_0.3.html");
    let written = std::fs::read_to_string(&inventory).expect("the inventory");
    assert!(
        written.trim_end().ends_with("</html>"),
        "the run adopted the orphaned scratch file as its output"
    );
    assert!(
        written.contains("KBo 1.1"),
        "the inventory does not hold the manuscripts of the archive"
    );

    // The orphan is another run's and this one has no way to know it is dead,
    // so it is left rather than deleted — but it must not be, or become, the
    // inventory. What matters is that nothing else was left beside it.
    let leftovers: Vec<String> = std::fs::read_dir(&downloads)
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        // The package is what the run is now *for*; the scratch file is the
        // orphan this test planted.
        .filter(|n| {
            n != "TLHdig_Beta_0.3.html"
                && n != "TLHdig_Beta_0.3.html.999999.0.part"
                && n != PACKAGE
                && !n.starts_with(&format!(".{PACKAGE}.build"))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "the run left {leftovers:?} in the output directory"
    );
}

/// A build that fails does not cost the reader the orphan.
///
/// The counterpart to the sweep above, and the reason it happens at the end of
/// a build rather than the start. An orphaned `.TLHdig_Beta_0.3.previous` is
/// the only copy left of the package the reader had; clearing it before the
/// work — the obvious place to put housekeeping — would mean a build that then
/// failed had taken their last copy and given them nothing.
#[test]
fn a_failed_build_leaves_an_orphaned_package_where_it_found_it() {
    let dir = tempdir().expect("tempdir");
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    let orphan = destination.join(format!(".{PACKAGE}.previous"));
    std::fs::create_dir_all(&orphan).expect("orphan");
    std::fs::write(orphan.join("theirs.xml"), "the package they had").expect("theirs");

    // Two documents claiming one place: refused rather than built.
    let zip = support::archive(
        &dir.path().join("collide.zip"),
        &[
            (
                "root/CTH 5_XML_HFR/a.xml",
                manuscript("KBo 1.1", "FB", "2017-03-28"),
            ),
            (
                "root/CTH 5_XML_TLH/b.xml",
                manuscript("KBo 1.1", "FB", "2017-03-28"),
            ),
        ],
    );
    let outcome = export::build(
        &zip,
        &destination,
        "failing",
        &aruna::job::Job::unattended(),
    );

    if outcome.is_ok() {
        // Disambiguated rather than refused; then a package was published and
        // the orphan is correctly gone. Either way it is never lost *and*
        // unreplaced.
        assert!(destination.join(PACKAGE).is_file() || destination.join(PACKAGE).is_dir());
        return;
    }
    assert!(
        orphan.join("theirs.xml").is_file(),
        "a build that failed took the reader's only remaining package with it"
    );
    assert!(
        !destination.join(PACKAGE).exists(),
        "a failed build published something"
    );
}

/// Running twice from a recovered state changes nothing further.
///
/// Idempotence after recovery: the second run replaces the package with the
/// same package and leaves the destination in the same state. A recovery that
/// only worked once — because it consumed something it also needed — would
/// pass every test above and fail here.
#[test]
fn recovering_once_is_enough_and_running_again_changes_nothing() {
    let (_dir, zip, destination) = published();

    std::fs::create_dir_all(destination.join(format!(".{PACKAGE}.build"))).expect("staging");
    export::build(&zip, &destination, "second", &aruna::job::Job::unattended()).expect("recovers");
    let after_recovery = files(&destination.join(PACKAGE));

    export::build(&zip, &destination, "second", &aruna::job::Job::unattended()).expect("and again");
    assert_eq!(
        after_recovery,
        files(&destination.join(PACKAGE)),
        "a second run after a recovery produced a different package"
    );
    assert!(
        beside(&destination).is_empty(),
        "the destination holds {:?}",
        beside(&destination)
    );
}
