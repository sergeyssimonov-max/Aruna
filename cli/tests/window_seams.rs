//! The seams a window will drive, held to what a window needs of them.
//!
//! None of this is interface work and none of it names Tauri. It is the part of
//! "a desktop application will sit on top of this" that is already a question
//! about reliability, and therefore already answerable:
//!
//! * the work runs on a thread that is not the caller's, and stops when the
//!   caller — on another thread — says so;
//! * the library says what it is doing through the sink it was given and
//!   through nothing else, and never ends the process;
//! * where the package goes is the caller's decision, not the environment's.
//!
//! Each is cheap to check now and impossible to retrofit once a window is
//! written against the current shape.

mod support;

use aruna::app::{self, CorpusRequest};
use aruna::error::ArunaError;
use aruna::export::{self, PACKAGE};
use aruna::job::{Cancel, Job};
use aruna::progress::{Event, Progress};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use support::{archive, manuscript};
use tempfile::tempdir;

/// An archive of `n` manuscripts over a few groups.
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
// Off the calling thread
// ---------------------------------------------------------------------------

/// A sink that stops at one stage and waits to be released.
///
/// The handshake is what makes the test deterministic rather than timed. A test
/// that cancelled after a sleep would be asserting how fast the machine is: on
/// a quick one the build finishes first and the run is not cancelled at all.
/// Here the worker announces the stage and blocks inside `report` until the
/// other thread has set the flag, so the next `job.check` is guaranteed to see
/// it — and what is proven is exactly the thing a window needs, that a flag set
/// on one thread stops work running on another.
struct Handshake {
    at: &'static str,
    reached: Sender<()>,
    /// Behind a `Mutex` because `Progress` is `Sync` and a `Receiver` is not.
    /// That requirement is the point rather than an inconvenience: a sink a
    /// window installs is read from whatever thread the work landed on, and
    /// the compiler is what holds every implementation to it.
    released: Mutex<Receiver<()>>,
}

impl Progress for Handshake {
    fn report(&self, event: Event<'_>) {
        let name = match event {
            Event::ReadingHeaders => "ReadingHeaders",
            Event::WritingDocuments { .. } => "WritingDocuments",
            _ => "other",
        };
        if name != self.at {
            return;
        }
        // Both sides are ignored on failure: once the other thread has stopped
        // listening, this run is on its way out and has nothing to say.
        let _ = self.reached.send(());
        if let Ok(released) = self.released.lock() {
            let _ = released.recv();
        }
    }
}

/// The build runs on a thread of its own and is stopped from the caller's.
#[test]
fn a_build_on_another_thread_is_stopped_from_this_one() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 40);
    let destination = dir.path().join("out");
    std::fs::create_dir(&destination).expect("destination");

    let cancel = Cancel::new();
    let (reached, at_the_stage) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let sink = Handshake {
        at: "WritingDocuments",
        reached,
        released: Mutex::new(released),
    };

    let outcome = std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            let job = Job::new(&sink, &cancel);
            export::build(&zip, &destination, "label", &job)
        });

        at_the_stage.recv().expect("the worker reached the stage");
        cancel.cancel();
        let _ = release.send(());

        worker.join().expect("the worker did not panic")
    });

    assert!(
        matches!(outcome, Err(ArunaError::Cancelled { .. })),
        "a flag set on this thread did not stop the work on the other: {outcome:?}"
    );
    assert_eq!(
        files(&destination),
        Vec::<PathBuf>::new(),
        "the build stopped from another thread left something behind"
    );
}

// ---------------------------------------------------------------------------
// Nothing printed, nothing exited
// ---------------------------------------------------------------------------

/// Whether `source` calls `name` — as a whole name, not as a tail of one.
///
/// `println!` is a suffix of `eprintln!`, so a plain substring search reported
/// the one file that is allowed to print as the one file that must not.
fn calls(source: &str, name: &str) -> bool {
    source.match_indices(name).any(|(at, _)| {
        at == 0
            || !source[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Every `.rs` file under `src`, with its path.
fn library_sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let relative = path.strip_prefix(&root).expect("under src").to_path_buf();
                out.push((relative, std::fs::read_to_string(&path).expect("read")));
            }
        }
    }
    assert!(out.len() > 10, "the source scan found almost nothing");
    out
}

/// The library says what it is doing through the sink and nowhere else, and it
/// never ends the process.
///
/// A grep, and deliberately so: the property is about the whole of `src`, and
/// there is no way to observe "nobody anywhere called `println!`" from a
/// running test. What it protects is the thing that makes a window possible at
/// all — a file descriptor is not something a window can show, and a library
/// that calls `process::exit` takes the application down with the operation.
///
/// Two files are exempt, each for one reason. `main.rs` is the front end: it is
/// what prints, and the whole point of the split is that it is the only one.
/// `progress.rs` holds `Stderr`, the sink the binary brings — the one place
/// where an event becomes a line, chosen by the caller rather than by the code
/// doing the work.
#[test]
fn the_library_neither_prints_nor_ends_the_process() {
    for (path, source) in library_sources() {
        let name = path.to_string_lossy();
        for forbidden in ["println!", "print!", "process::exit", "process::abort"] {
            assert!(
                !calls(&source, forbidden) || name == "main.rs",
                "{name} contains {forbidden}: a library that writes to a file \
                 descriptor or ends the process cannot be driven by a window"
            );
        }
        assert!(
            !calls(&source, "eprintln!") || name == "main.rs" || name == "progress.rs",
            "{name} contains eprintln!: report an Event through the job instead, \
             and let the caller decide where it goes"
        );
    }
}

// ---------------------------------------------------------------------------
// The destination is the caller's to name
// ---------------------------------------------------------------------------

/// The scenario builds where it is told, not where the environment says.
///
/// `build_corpus` still answers "wherever this platform keeps downloads", which
/// is right for the binary and wrong for anything else: a window that lets
/// someone choose a folder had no way to say so, and a test had to move `HOME`
/// to keep the corpus out of the real Downloads directory. The destination is
/// now an argument, and this is what says the argument is honoured.
#[test]
fn the_corpus_is_built_where_the_caller_says() {
    let dir = tempdir().expect("tempdir");
    let zip = corpus(dir.path(), 12);
    let destination = dir.path().join("chosen");
    std::fs::create_dir(&destination).expect("destination");

    let report = app::build_corpus_into(
        &CorpusRequest {
            local_archive: Some(zip),
        },
        &destination,
        &Job::unattended(),
    )
    .expect("builds");

    assert_eq!(report.package.root, destination.join(PACKAGE));
    assert!(report.inventory.is_file(), "the inventory is where it says");
    assert!(
        report.inventory.starts_with(&destination),
        "the inventory landed outside the folder the caller named"
    );

    // And nowhere near the folder the environment would have chosen.
    if let Ok(downloads) = aruna::paths::downloads_dir() {
        assert!(
            !report.package.root.starts_with(&downloads),
            "the caller named a folder and the package went to {}",
            downloads.display()
        );
    }
}
