//! Stopping a run, and what it leaves behind.
//!
//! Cancellation is easy to add and easy to add uselessly. A flag that is
//! checked once before the work starts satisfies every type signature in the
//! crate and stops nothing; a flag checked in the wrong place stops the work
//! half way through writing a document. Neither is visible from the outside
//! without asking the two questions this file asks:
//!
//! 1. **Does it stop?** Measured by counting — the archive is asked for a
//!    document and the count is compared against what a complete run produces.
//!    A test that only checked the error would pass on an implementation that
//!    ran to the end and reported a cancellation afterwards.
//! 2. **What is left?** Nothing. A cancelled build must leave the destination
//!    exactly as it found it, which is the guarantee a failed build already
//!    had — and reaching it by the same mechanism, rather than by a second
//!    cleanup path, is why that is credible.
//!
//! No sleeps and no timing. Every test here cancels from a progress sink, at a
//! stage the run reports reaching — so the cancellation lands at a known point
//! in the work rather than at whatever point a timer happened to fire on a
//! loaded machine.

mod support;

use aruna::error::ArunaError;
use aruna::export::{self, PACKAGE};
use aruna::job::{Cancel, Job, Phase};
use aruna::progress::{Event, Progress};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use support::{archive, manuscript};
use tempfile::{tempdir, TempDir};

/// A sink that cancels the run when it hears a stage it was told to wait for.
///
/// The alternative — cancelling from another thread after a delay — makes the
/// point in the work at which the flag is seen depend on how busy the machine
/// is, which is how a test becomes flaky and how a defect hides.
struct CancelAt {
    at: &'static str,
    cancel: Cancel,
    seen: Mutex<Vec<&'static str>>,
    fired: AtomicUsize,
}

impl CancelAt {
    fn new(at: &'static str, cancel: &Cancel) -> CancelAt {
        CancelAt {
            at,
            cancel: cancel.clone(),
            seen: Mutex::new(Vec::new()),
            fired: AtomicUsize::new(0),
        }
    }

    fn stages(&self) -> Vec<&'static str> {
        self.seen.lock().expect("not poisoned").clone()
    }
}

impl Progress for CancelAt {
    fn report(&self, event: Event<'_>) {
        let name = match event {
            Event::ParsingArchive => "ParsingArchive",
            Event::Indexed { .. } => "Indexed",
            Event::ReadingHeaders => "ReadingHeaders",
            Event::HeadersRead { .. } => "HeadersRead",
            Event::WritingDocuments { .. } => "WritingDocuments",
            Event::CheckingPackage => "CheckingPackage",
            Event::CheckingPublished => "CheckingPublished",
            _ => "other",
        };
        self.seen.lock().expect("not poisoned").push(name);
        if name == self.at {
            self.fired.fetch_add(1, Ordering::SeqCst);
            self.cancel.cancel();
        }
    }
}

/// An archive of `n` manuscripts, spread over a few groups.
fn corpus(dir: &Path, n: usize) -> PathBuf {
    let entries: Vec<(String, String)> = (0..n)
        .map(|i| {
            (
                format!("root/CTH {}_XML_HFR/doc {i}.xml", i % 5),
                manuscript(&format!("KBo {i}"), "FB", "2017-03-28"),
            )
        })
        .collect();
    let borrowed: Vec<(&str, String)> = entries
        .iter()
        .map(|(path, body)| (path.as_str(), body.clone()))
        .collect();
    archive(&dir.join("corpus.zip"), &borrowed)
}

/// A destination, and the archive to build into it.
fn scene(documents: usize) -> (TempDir, PathBuf, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), documents);
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");
    (dir, zip, destination)
}

/// Everything under `root`, relative to it.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
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

// ---------------------------------------------------------------------------

/// A build cancelled before it writes writes nothing.
#[test]
fn a_build_cancelled_at_the_first_stage_writes_no_documents() {
    let (_dir, zip, destination) = scene(40);
    let cancel = Cancel::new();
    let sink = CancelAt::new("ReadingHeaders", &cancel);

    let outcome = export::build(&zip, &destination, "cancelled", &Job::new(&sink, &cancel));

    assert!(
        matches!(outcome, Err(ArunaError::Cancelled { .. })),
        "a cancelled build reported {outcome:?}"
    );
    assert!(
        files(&destination).is_empty(),
        "the destination holds {:?}",
        files(&destination)
    );
    assert!(
        !sink.stages().contains(&"CheckingPublished"),
        "the build went on to publish: {:?}",
        sink.stages()
    );
}

/// A build cancelled while writing stops, and takes its half-built package
/// with it.
///
/// The staging directory is what makes this true: the work happens under a
/// hidden name and takes the real one only at the end, so stopping is the same
/// as failing, and the cleanup is the `Drop` that was already there.
#[test]
fn a_build_cancelled_while_writing_leaves_the_destination_empty() {
    let (_dir, zip, destination) = scene(60);
    let cancel = Cancel::new();
    let sink = CancelAt::new("WritingDocuments", &cancel);

    let outcome = export::build(&zip, &destination, "cancelled", &Job::new(&sink, &cancel));

    assert!(matches!(
        outcome,
        Err(ArunaError::Cancelled {
            phase: Phase::Exporting
        })
    ));
    assert!(
        files(&destination).is_empty(),
        "a cancelled build left {:?} behind",
        files(&destination)
    );
    assert!(
        !destination.join(PACKAGE).exists(),
        "a cancelled build published a package"
    );
    // The staging directory went with it: nothing hidden is left either.
    let leftovers: Vec<String> = std::fs::read_dir(&destination)
        .expect("read")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(leftovers.is_empty(), "hidden leftovers: {leftovers:?}");
}

/// The package a reader already has survives a cancelled rebuild.
///
/// The case that matters most. Someone has a package, starts a rebuild, changes
/// their mind — and must still have the package they started with. Not a
/// half-replaced one, and not none.
#[test]
fn a_cancelled_rebuild_leaves_the_existing_package_untouched() {
    let (_dir, zip, destination) = scene(40);

    export::build(&zip, &destination, "first", &Job::unattended()).expect("the first build");
    let before = files(&destination);
    let inventory = std::fs::read(destination.join(PACKAGE).join(format!("{PACKAGE}.html")))
        .expect("the first inventory");

    let cancel = Cancel::new();
    let sink = CancelAt::new("WritingDocuments", &cancel);
    let outcome = export::build(&zip, &destination, "second", &Job::new(&sink, &cancel));
    assert!(matches!(outcome, Err(ArunaError::Cancelled { .. })));

    assert_eq!(
        files(&destination),
        before,
        "the package the reader had is not the package they have"
    );
    assert_eq!(
        std::fs::read(destination.join(PACKAGE).join(format!("{PACKAGE}.html")))
            .expect("the inventory"),
        inventory,
        "the inventory changed under a build that was cancelled"
    );
}

/// Stopping is not a state the program has to be restarted out of.
///
/// The clause a window depends on: a person clicks *Cancel*, then clicks
/// *Build* again, and the second run has to produce the whole package — in the
/// same process, with no leftover to trip over and no memory of the first
/// attempt. `a_cancelled_rebuild_leaves_the_existing_package_untouched` is the
/// other half, and stops where the cancellation does.
///
/// Three things are asserted, and the first is the one that would fail
/// quietly: after the cancelled run the destination holds *nothing* — no
/// staging directory, no `.previous`, no partial package. A leftover would not
/// break the second run, it would sit beside its output for good.
#[test]
fn a_run_stopped_half_way_is_followed_by_a_complete_one() {
    let (_dir, zip, destination) = scene(40);
    let elsewhere = tempdir().expect("tempdir");

    let cancel = Cancel::new();
    let sink = CancelAt::new("WritingDocuments", &cancel);
    let stopped = export::build(&zip, &destination, "label", &Job::new(&sink, &cancel));
    assert!(matches!(stopped, Err(ArunaError::Cancelled { .. })));
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "the cancelled run left something behind in the destination"
    );

    // The same process, a fresh handle, and nothing said about the first
    // attempt.
    let again = export::build(&zip, &destination, "label", &Job::unattended())
        .expect("the second run builds");
    let reference = export::build(&zip, elsewhere.path(), "label", &Job::unattended())
        .expect("a run that was never stopped");
    assert_eq!(again.documents, reference.documents);
    assert_eq!(again.groups, reference.groups);

    let (after, never) = (destination.join(PACKAGE), elsewhere.path().join(PACKAGE));
    assert_eq!(
        files(&after),
        files(&never),
        "the package built after a cancellation holds different files"
    );
    for relative in files(&after) {
        assert_eq!(
            std::fs::read(after.join(&relative)).expect("after"),
            std::fs::read(never.join(&relative)).expect("never"),
            "{} differs from what a run that was never stopped writes",
            relative.display()
        );
    }
    assert_eq!(
        files(&destination),
        files(&after)
            .iter()
            .map(|relative| PathBuf::from(PACKAGE).join(relative))
            .collect::<Vec<_>>(),
        "the second run published beside something the first one left"
    );
}

/// The parse stops inside its loop rather than finishing and reporting
/// afterwards.
///
/// `parse_zip` emits no event per entry — deliberately, and
/// `progress_flow.rs::the_number_of_events_does_not_grow_with_the_archive` is
/// what keeps it that way — so there is no event to cancel from and no
/// deterministic way to land the flag on entry 100 rather than entry 3.
/// Cancelling before the call lands it on entry 0, which exercises the same
/// single check the loop makes on every iteration.
///
/// What that establishes, and what it does not: the loop is interruptible and
/// stops without producing records. It does not establish *where* in a long
/// archive a mid-flight cancellation lands, which is a property of one atomic
/// load and not something a test can pin without a clock.
#[test]
fn a_cancelled_parse_stops_inside_its_loop() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 200);

    let complete = aruna::archive::parse_zip(&zip, &Job::unattended()).expect("a complete parse");
    assert_eq!(complete.len(), 200, "the fixture is the size it claims");

    let cancel = Cancel::new();
    cancel.cancel();
    let outcome = aruna::archive::parse_zip(&zip, &Job::new(&aruna::progress::Silent, &cancel));

    match outcome {
        Err(ArunaError::Cancelled {
            phase: Phase::Parsing,
        }) => {}
        Ok(records) => panic!(
            "the parse ran to completion under a cancelled job and returned {} records",
            records.len()
        ),
        other => panic!("the parse reported {other:?} rather than a cancellation"),
    }
}

/// The whole run stops, from the top.
///
/// `run` is the CLI's entry point and the one a window will drive; a
/// cancellation has to travel out of it as a cancellation rather than as
/// whatever the stage it interrupted would have said.
#[test]
fn a_cancelled_run_writes_no_inventory() {
    let home = tempdir().expect("tempdir");
    let downloads = home.path().join("Downloads");
    std::fs::create_dir_all(&downloads).expect("downloads");
    let zip = corpus(home.path(), 30);

    let cancel = Cancel::new();
    cancel.cancel();
    let outcome = aruna::run(Some(&zip), &Job::new(&aruna::progress::Silent, &cancel));

    assert!(
        matches!(outcome, Err(ArunaError::Cancelled { .. })),
        "a cancelled run reported {outcome:?}"
    );
    assert!(
        files(&downloads).is_empty(),
        "a cancelled run left {:?} in the output directory",
        files(&downloads)
    );
}

/// Cancelling twice is cancelling once, and a run cancelled after it finished
/// is not retroactively a failure.
#[test]
fn a_finished_run_is_not_undone_by_a_late_cancellation() {
    let (_dir, zip, destination) = scene(20);
    let cancel = Cancel::new();

    let built = export::build(
        &zip,
        &destination,
        "done",
        &Job::new(&aruna::progress::Silent, &cancel),
    )
    .expect("the build finishes");
    assert_eq!(built.documents, 20);

    // The window's Cancel button, pressed a moment too late.
    cancel.cancel();
    cancel.cancel();

    assert!(
        destination.join(PACKAGE).is_dir(),
        "a finished package disappeared when a late cancellation arrived"
    );
    assert!(destination
        .join(PACKAGE)
        .join(format!("{PACKAGE}.html"))
        .is_file());
}

/// A run that is never cancelled is not slowed into a different program.
///
/// The check is one relaxed atomic load per document. This does not measure it
/// — a timing assertion on a laptop is a flaky test — it asserts the thing that
/// would actually be wrong: that an uncancelled run still produces the whole
/// package, byte for byte identical to one built by a job that has no
/// cancellation handle at all.
#[test]
fn a_run_that_is_never_cancelled_produces_exactly_what_it_did_before() {
    let (_dir, zip, destination) = scene(30);
    let elsewhere = tempdir().expect("tempdir");

    let cancel = Cancel::new();
    let watched = export::build(
        &zip,
        &destination,
        "watched",
        &Job::new(&aruna::progress::Silent, &cancel),
    )
    .expect("built");
    let unwatched =
        export::build(&zip, elsewhere.path(), "watched", &Job::unattended()).expect("built");

    assert_eq!(watched.documents, unwatched.documents);
    assert_eq!(watched.groups, unwatched.groups);

    let (a, b) = (destination.join(PACKAGE), elsewhere.path().join(PACKAGE));
    assert_eq!(files(&a), files(&b));
    for relative in files(&a) {
        assert_eq!(
            std::fs::read(a.join(&relative)).expect("watched"),
            std::fs::read(b.join(&relative)).expect("unwatched"),
            "{} differs between a cancellable run and an unattended one",
            relative.display()
        );
    }
}

/// The cancellation names the stage it happened in.
///
/// A window shows it and a log records it; a cancellation that could not say
/// where it happened would be indistinguishable from any other.
#[test]
fn a_cancellation_says_which_stage_it_stopped() {
    let (_dir, zip, destination) = scene(40);

    for (at, expected) in [
        ("ReadingHeaders", Phase::Exporting),
        ("WritingDocuments", Phase::Exporting),
    ] {
        let cancel = Cancel::new();
        let sink = CancelAt::new(at, &cancel);
        let outcome = export::build(&zip, &destination, "x", &Job::new(&sink, &cancel));
        match outcome {
            Err(ArunaError::Cancelled { phase }) => assert_eq!(
                phase, expected,
                "cancelled at {at} and reported phase {phase}"
            ),
            other => panic!("cancelled at {at} gave {other:?}"),
        }
    }
}

/// The binary is a caller that never cancels, and nothing about it changed.
///
/// The flag exists so a window can stop the work. A terminal program has no
/// Cancel button, creates the handle, never sets it, and must behave exactly as
/// it did — this is the assertion that the mechanism cost the CLI nothing.
#[test]
fn the_binary_still_runs_to_completion() {
    let home = tempdir().expect("tempdir");
    std::fs::create_dir_all(home.path().join("Downloads")).expect("downloads");
    let zip = corpus(home.path(), 12);

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_aruna"))
        .env("HOME", home.path())
        .env("ARUNA_ZIP", &zip)
        .env("ARUNA_CACHE_DIR", home.path().join("cache"))
        .env_remove("XDG_CACHE_HOME")
        .env_remove("XDG_DOWNLOAD_DIR")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("the binary runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the run failed:\n{stderr}");
    assert!(
        !stderr.contains("Остановлено"),
        "an uncancelled run reported itself stopped:\n{stderr}"
    );
    assert!(home
        .path()
        .join("Downloads")
        .join("TLHdig_Beta_0.3")
        .join("TLHdig_Beta_0.3.html")
        .is_file());
}
