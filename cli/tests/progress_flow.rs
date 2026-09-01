//! What a run tells the caller it is doing, as a sequence rather than as
//! wording.
//!
//! `progress::Event`'s own tests pin how each event reads. Nothing pinned which
//! events a real run produces, in what order, or that their numbers agree with
//! what the run returns — and those are the properties a window depends on. A
//! progress bar driven by `WritingDocuments { documents }` and finished by
//! `CheckingPublished` is wrong in a way no wording test can see if the build
//! reports 23 936 and writes 23 935, or reports the stages out of order, or
//! reports `CheckingPublished` on a build that failed.
//!
//! Three properties, and the third is the one that matters for a GUI:
//!
//! 1. the stages arrive in the order the pipeline runs them;
//! 2. their numbers are the numbers the call returns;
//! 3. the number of events does not grow with the corpus.
//!
//! (3) is a resource property, not a cosmetic one. Seventeen call sites that
//! became seventeen thousand — one per document — would be a formatted string,
//! a lock and a write per manuscript in the hot path, and over an IPC channel
//! it would be 24 000 messages a window has to drain while it is trying to
//! paint. The core reports stages, and this holds it to that.

mod support;

use aruna::export::{self, Built};
use aruna::job::{Cancel, Job};
use aruna::progress::{Event, Progress};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use support::{archive, manuscript, mixed_archive};
use tempfile::tempdir;

/// One event, reduced to what this file asserts on: which stage, and the
/// numbers it carried.
///
/// Deliberately not `Event` itself — that borrows, and a recording sink has to
/// outlive the call. Reducing it here also means a new event with new wording
/// does not touch this file, while a new *stage* does.
#[derive(Debug, PartialEq, Eq, Clone)]
enum Seen {
    ParsingArchive,
    EntriesSkipped {
        by_path: usize,
        by_content: usize,
    },
    Indexed(usize),
    ReadingHeaders,
    HeadersRead {
        manuscripts: usize,
        groups: usize,
    },
    WritingDocuments(usize),
    CheckingPackage,
    CheckingPublished,
    Other(&'static str),
    /// A refinement of a stage rather than a stage — `Event::is_tick`. Kept
    /// apart from `Other` so a run's milestones can be read without the
    /// forty-eight fractions of the write pass in among them.
    Tick(&'static str),
    /// The write pass's refinement, with its numbers: this file asserts on how
    /// often it arrives and what it says, not only that it did.
    Written {
        done: usize,
        total: usize,
    },
}

impl Seen {
    /// The stage's name, for order assertions that do not care about counts.
    fn stage(&self) -> &'static str {
        match self {
            Seen::ParsingArchive => "ParsingArchive",
            Seen::EntriesSkipped { .. } => "EntriesSkipped",
            Seen::Indexed(_) => "Indexed",
            Seen::ReadingHeaders => "ReadingHeaders",
            Seen::HeadersRead { .. } => "HeadersRead",
            Seen::WritingDocuments(_) => "WritingDocuments",
            Seen::CheckingPackage => "CheckingPackage",
            Seen::CheckingPublished => "CheckingPublished",
            Seen::Other(name) => name,
            Seen::Tick(name) => name,
            Seen::Written { .. } => "DocumentsWritten",
        }
    }
}

/// A sink that keeps what it was told, in order.
#[derive(Default)]
struct Recording(Mutex<Vec<Seen>>);

impl Progress for Recording {
    fn report(&self, event: Event<'_>) {
        let seen = match event {
            Event::ParsingArchive => Seen::ParsingArchive,
            Event::EntriesSkipped {
                by_path,
                by_content,
            } => Seen::EntriesSkipped {
                by_path,
                by_content,
            },
            Event::Indexed { manuscripts } => Seen::Indexed(manuscripts),
            Event::ReadingHeaders => Seen::ReadingHeaders,
            Event::HeadersRead {
                manuscripts,
                groups,
            } => Seen::HeadersRead {
                manuscripts,
                groups,
            },
            Event::WritingDocuments { documents } => Seen::WritingDocuments(documents),
            Event::CheckingPackage => Seen::CheckingPackage,
            Event::CheckingPublished => Seen::CheckingPublished,
            Event::CacheUnusable { .. } => Seen::Other("CacheUnusable"),
            Event::CachedArchiveRejected => Seen::Other("CachedArchiveRejected"),
            Event::ArchiveFromCache { .. } => Seen::Other("ArchiveFromCache"),
            Event::ZenodoNotice { .. } => Seen::Other("ZenodoNotice"),
            Event::ZenodoUnreachable { .. } => Seen::Other("ZenodoUnreachable"),
            Event::DownloadStarted => Seen::Other("DownloadStarted"),
            Event::DownloadRetrying { .. } => Seen::Other("DownloadRetrying"),
            Event::ArchiveKept { .. } => Seen::Other("ArchiveKept"),
            Event::PreviousPackageLeft { .. } => Seen::Other("PreviousPackageLeft"),
            // Ticks, and this file is about the order of the stages. They are
            // kept apart by name so `the_stages_come_in_the_one_order` reads a
            // sequence of milestones rather than one interleaved with however
            // many refinements the corpus happened to produce.
            Event::Downloading { .. } => Seen::Tick("Downloading"),
            Event::DocumentsWritten { done, total } => Seen::Written { done, total },
        };
        self.0.lock().expect("not poisoned").push(seen);
    }
}

impl Recording {
    fn events(&self) -> Vec<Seen> {
        self.0.lock().expect("not poisoned").clone()
    }

    /// The milestones, without the refinements between them.
    fn stages(&self) -> Vec<&'static str> {
        self.events()
            .iter()
            .filter(|seen| !matches!(seen, Seen::Tick(_) | Seen::Written { .. }))
            .map(Seen::stage)
            .collect()
    }

    /// Only the refinements, with the two halves they carried.
    fn ticks(&self) -> Vec<(usize, usize)> {
        self.0
            .lock()
            .expect("not poisoned")
            .iter()
            .filter_map(|seen| match seen {
                Seen::Written { done, total } => Some((*done, *total)),
                _ => None,
            })
            .collect()
    }
}

/// Build under a fresh destination and return what was built and what was said.
fn build_recording(zip: &Path) -> (Built, Recording) {
    let dir = tempdir().expect("tempdir");
    let sink = Recording::default();
    let cancel = Cancel::new();
    let built = export::build(zip, dir.path(), "progress", &Job::new(&sink, &cancel))
        .expect("the package builds");
    // The tempdir is dropped here on purpose: the assertions are about what was
    // reported, and the package itself is other files' business.
    (built, sink)
}

/// The export's stages arrive in the order the pipeline runs them.
///
/// Asserted as the whole sequence rather than a subsequence: unlike the parse,
/// every one of these five is unconditional, so an extra or missing stage is a
/// change to the pipeline and should be read as one.
#[test]
fn the_export_reports_its_stages_in_the_order_it_runs_them() {
    let dir = tempdir().expect("tempdir");
    let (_, sink) = build_recording(&mixed_archive(dir.path()));

    assert_eq!(
        sink.stages(),
        [
            "ReadingHeaders",
            "HeadersRead",
            "WritingDocuments",
            "CheckingPackage",
            "CheckingPublished",
        ],
        "the export's progress no longer describes the pipeline it runs"
    );
}

/// The numbers in the report are the numbers the call returns.
///
/// `HeadersRead` is what a window sizes its progress bar from, and it is
/// computed before the documents are placed — from `fragments`, by a different
/// counter than the `Built` the call ends with. Those two disagreeing by 163
/// groups is a defect this project has already had once.
#[test]
fn the_counts_reported_are_the_counts_returned() {
    let dir = tempdir().expect("tempdir");
    let (built, sink) = build_recording(&mixed_archive(dir.path()));

    let events = sink.events();
    let headers = events
        .iter()
        .find_map(|e| match e {
            Seen::HeadersRead {
                manuscripts,
                groups,
            } => Some((*manuscripts, *groups)),
            _ => None,
        })
        .expect("the export reports what its headers came to");
    let writing = events
        .iter()
        .find_map(|e| match e {
            Seen::WritingDocuments(n) => Some(*n),
            _ => None,
        })
        .expect("the export reports how many documents it is writing");

    assert_eq!(
        headers,
        (built.documents, built.groups),
        "the progress line and the result disagree about what was built"
    );
    assert_eq!(
        writing, built.documents,
        "the export announced a different number of documents than it placed"
    );
}

/// A build that fails does not report the stages it never reached.
///
/// The last thing a window hears has to be true. `CheckingPublished` after a
/// failure would leave a progress bar full and a package absent, which is worse
/// than a bar that stops where the work did.
#[test]
fn a_failed_build_does_not_report_stages_it_never_reached() {
    let dir = tempdir().expect("tempdir");
    // Two documents claiming one path: the collision is refused during the
    // write, after the headers are read and announced.
    let zip = archive(
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
    let destination = tempdir().expect("tempdir");
    let sink = Recording::default();
    let cancel = Cancel::new();
    let outcome = export::build(
        &zip,
        destination.path(),
        "progress",
        &Job::new(&sink, &cancel),
    );

    let stages = sink.stages();
    if outcome.is_ok() {
        // The two are disambiguated rather than refused — then the run really
        // did reach the end, and the sequence must say exactly that.
        assert_eq!(stages.last(), Some(&"CheckingPublished"));
        return;
    }
    assert!(
        !stages.contains(&"CheckingPublished"),
        "a build that failed still announced that it had checked the published copy: {stages:?}"
    );
    assert!(
        stages.starts_with(&["ReadingHeaders", "HeadersRead"]),
        "the stages before the failure are missing: {stages:?}"
    );
}

/// The parse says it started before it says what it found.
///
/// A subsequence rather than the whole list: `EntriesSkipped` is reported only
/// when the archive carried debris, and pinning it unconditionally would make
/// this fail on a clean archive for no reason.
#[test]
fn the_parse_reports_what_it_found_after_it_reports_starting() {
    let dir = tempdir().expect("tempdir");
    let zip = mixed_archive(dir.path());
    let sink = Recording::default();

    // `parse_zip` is the step `run` reports around, so the two events that
    // bracket it are produced by the caller; what this checks is the one the
    // parse itself emits, and that the count it reports is the corpus.
    let cancel = Cancel::new();
    let records =
        aruna::archive::parse_zip(&zip, &Job::new(&sink, &cancel)).expect("the archive parses");

    let skipped = sink
        .events()
        .into_iter()
        .find_map(|e| match e {
            Seen::EntriesSkipped {
                by_path,
                by_content,
            } => Some((by_path, by_content)),
            _ => None,
        })
        .expect("the archive carries debris, so the parse reports it");
    assert!(
        skipped.0 + skipped.1 > 0,
        "the parse reported skipping nothing, which it does not report at all"
    );
    assert!(!records.is_empty());
}

/// The number of *stages* does not grow with the corpus.
///
/// Twelve times the documents, the same number of milestones. This is what keeps
/// progress out of the hot path: a per-document event would be a formatted
/// string and a channel send 24 000 times, and a window would spend the build
/// draining messages instead of painting.
///
/// The write pass also refines the stage it is in, and those refinements do grow
/// — by design and at one five-hundredth of the rate. They are counted by
/// [`the_write_is_refined_in_batches_and_ends_on_the_whole`] instead, which is
/// where the batching promise belongs.
#[test]
fn the_number_of_stages_does_not_grow_with_the_archive() {
    let dir = tempdir().expect("tempdir");

    let count_for = |documents: usize, name: &str| {
        let entries: Vec<(String, String)> = (0..documents)
            .map(|i| {
                (
                    format!("root/CTH {}_XML_HFR/doc {i}.xml", i % 7),
                    manuscript(&format!("KBo {i}"), "FB", "2017-03-28"),
                )
            })
            .collect();
        let borrowed: Vec<(&str, String)> = entries
            .iter()
            .map(|(path, body)| (path.as_str(), body.clone()))
            .collect();
        let zip = archive(&dir.path().join(name), &borrowed);
        let (built, sink) = build_recording(&zip);
        assert_eq!(built.documents, documents);
        sink.stages().len()
    };

    let small = count_for(5, "small.zip");
    let large = count_for(60, "large.zip");
    assert_eq!(
        small, large,
        "the export reported {small} stages for 5 documents and {large} for 60 — \
         progress is being emitted per document, not per stage"
    );
}

/// The write pass says how far it has got, in batches, and its last word is the
/// whole of it.
///
/// Both halves of the promise `docs/FRONTEND-CONTRACT.md` §3 asks to be stated.
/// One every five hundred documents is what keeps 23 936 documents to
/// forty-eight messages instead of 23 936; ending on `done == total` is what
/// lets a window finish its bar on the number the report will carry, rather than
/// leaving it at 500 short and jumping.
#[test]
fn the_write_is_refined_in_batches_and_ends_on_the_whole() {
    let dir = tempdir().expect("tempdir");
    let entries: Vec<(String, String)> = (0..600)
        .map(|i| {
            (
                format!("root/CTH {}_XML_HFR/doc {i}.xml", i % 7),
                manuscript(&format!("KBo {i}"), "FB", "2017-03-28"),
            )
        })
        .collect();
    let borrowed: Vec<(&str, String)> = entries
        .iter()
        .map(|(path, body)| (path.as_str(), body.clone()))
        .collect();
    let zip = archive(&dir.path().join("six-hundred.zip"), &borrowed);
    let (built, sink) = build_recording(&zip);
    assert_eq!(built.documents, 600);

    assert_eq!(
        sink.ticks(),
        [(500, 600), (600, 600)],
        "the write pass no longer reports in batches of five hundred, or no \
         longer ends on the whole"
    );
}

/// The binary says the same things on stderr, in the same order.
///
/// The in-process assertions above drive the library directly; this one is the
/// path a person actually runs, and it is the one that proves `progress::Stderr`
/// is wired into the binary rather than merely existing. Run as a child process
/// so no environment variable is mutated inside a test process shared with
/// others.
#[test]
fn the_binary_reports_the_same_run_on_stderr() {
    let home = tempdir().expect("tempdir");
    std::fs::create_dir_all(home.path().join("Downloads")).expect("downloads");
    let zip = mixed_archive(home.path());

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

    // The export's own first two stages, in order. The run used to open with
    // "Parsing XML manuscripts" — that was the standalone inventory's phase,
    // and there is no standalone inventory since 2.3.0.
    let started = stderr
        .find("Reading headers")
        .expect("the run says it started reading");
    let counted = stderr
        .find(" manuscripts in ")
        .expect("the run says what it found");
    assert!(
        started < counted,
        "the run reported what it found before it reported starting:\n{stderr}"
    );
    assert!(!stderr.contains("panicked"), "the run panicked:\n{stderr}");
}
