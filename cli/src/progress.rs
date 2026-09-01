//! What a run is doing, told to whoever is listening.
//!
//! The core used to say it by printing: seventeen `eprintln!` calls across six
//! modules, each deciding on its own that stderr existed and that someone was
//! reading it. That is fine for a program with one front end and fatal for a
//! library — a window cannot show a file descriptor, and two conversions
//! running at once in one process cannot share it.
//!
//! So the fact and the saying of it are split. The core reports an [`Event`] —
//! a thing that happened, with the numbers and paths that make it worth
//! hearing — and the caller brings a [`Progress`] to receive it. The CLI brings
//! [`Stderr`], which prints exactly what the seventeen printed; tests bring
//! [`Silent`]; a GUI brings its own, and never has to parse a line of English
//! back into a number.
//!
//! [`Event`] is deliberately *not* `#[non_exhaustive]`. A front end that has
//! not been taught about a new stage should fail to compile rather than fall
//! through a wildcard arm and show the reader nothing — the same reason
//! `main::advice` keeps one arm per error.

use crate::error::ArunaError;
use std::fmt;
use std::path::Path;
use std::time::Duration;

/// Something the run did that is worth telling the caller about.
///
/// Borrows rather than owns: every one of these is reported from inside the
/// loop or the step that produced it, and a sink that needs to keep a value
/// past the call can copy the parts it wants. That keeps the cost of a report
/// nothing at all for [`Silent`], which is what the test suite runs with.
#[derive(Debug)]
pub enum Event<'a> {
    /// The cache directory cannot be written to, so this run does without one.
    CacheUnusable { dir: &'a Path },
    /// A cached archive was there under the right name and hashed to something
    /// else, so it is being fetched again.
    CachedArchiveRejected,
    /// The archive was already downloaded and still hashes as promised.
    ArchiveFromCache { path: &'a Path },
    /// Zenodo says something about the record that differs from what this build
    /// expects. Prose, because [`crate::zenodo::advice`] composes it and its
    /// wording is what the test suite pins.
    ZenodoNotice { message: &'a str },
    /// The record could not be checked. Advisory: the download goes ahead.
    ZenodoUnreachable { cause: &'a str },
    /// The 71 MiB are on their way.
    DownloadStarted,
    /// How much of the archive has arrived. A tick, not a stage — see
    /// [`Event::is_tick`].
    ///
    /// `total` is what the server announced, and it is `None` when it announced
    /// nothing: a fraction of an unknown whole is not a number to show, and
    /// inventing one would be the interface saying more than the transfer knows.
    Downloading { bytes: u64, total: Option<u64> },
    /// An attempt failed in a way another attempt can fix.
    DownloadRetrying {
        attempt: u32,
        delay: Duration,
        error: &'a ArunaError,
    },
    /// The downloaded archive is in the cache, and the next run will not pay
    /// for it.
    ArchiveKept { path: &'a Path },
    /// The archive is open and its manuscripts are being read.
    ParsingArchive,
    /// Entries the two gates turned away, counted apart: junk the archive has
    /// always carried, and documents named like manuscripts that are not.
    EntriesSkipped { by_path: usize, by_content: usize },
    /// The parse is done and this is what it found.
    Indexed { manuscripts: usize },
    /// The export is reading header windows out of the archive.
    ReadingHeaders,
    /// What those headers came to, before a byte of the package is written.
    HeadersRead { manuscripts: usize, groups: usize },
    /// The package is being written into its staging directory.
    WritingDocuments { documents: usize },
    /// How many of them are written. A tick, not a stage — see
    /// [`Event::is_tick`].
    ///
    /// Both halves, so that a sink showing a fraction cannot show one whose
    /// denominator drifted from the stage line above it.
    DocumentsWritten { done: usize, total: usize },
    /// The staged package is being checked against the model it came from.
    CheckingPackage,
    /// The same check again, on the copy that was published.
    CheckingPublished,
    /// The package this build replaced could not be removed and is still there.
    PreviousPackageLeft { path: &'a Path },
}

impl Event<'_> {
    /// Whether this is a refinement of a stage rather than a stage.
    ///
    /// A tick says how far along the stage already announced is: two of the
    /// seventeen milestones take minutes and used to pass in silence, and an
    /// interface that shows a run has to show them moving. They are told apart
    /// here rather than at each sink, because "does this deserve a line" is one
    /// question with one answer: a line-oriented sink drops them — fifty of
    /// them under `Writing 23936 documents…` would bury the sentence they
    /// refine — and a sink drawing a window is exactly what they are for.
    ///
    /// The rate they arrive at is the reporter's business and is documented
    /// where they are sent: a quarter-second for the transfer, five hundred
    /// documents for the write.
    pub fn is_tick(&self) -> bool {
        matches!(
            self,
            Event::Downloading { .. } | Event::DocumentsWritten { .. }
        )
    }
}

/// The CLI's wording, kept on the event rather than inside the printing.
///
/// One place decides how a stage reads, a test can read it back without
/// capturing a file descriptor, and [`Stderr`] is left with nothing to do but
/// print. The same separation `main::advice` has from `main::report`, and for
/// the same reason.
impl fmt::Display for Event<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::CacheUnusable { dir } => write!(
                f,
                "Cannot write to the cache directory ({}); downloading for this run only.",
                dir.display()
            ),
            Event::CachedArchiveRejected => {
                write!(
                    f,
                    "Cached archive failed its checksum; downloading it again."
                )
            }
            Event::ArchiveFromCache { path } => {
                write!(
                    f,
                    "Using the archive already downloaded: {}",
                    path.display()
                )
            }
            Event::ZenodoNotice { message } => write!(f, "{message}"),
            Event::ZenodoUnreachable { cause } => {
                write!(
                    f,
                    "Could not check the record on Zenodo ({cause}); continuing."
                )
            }
            Event::DownloadStarted => write!(f, "Downloading TLHdig archive from Zenodo…"),
            Event::Downloading {
                bytes,
                total: Some(total),
            } => write!(f, "  {bytes} of {total} bytes"),
            Event::Downloading { bytes, total: None } => write!(f, "  {bytes} bytes"),
            Event::DownloadRetrying {
                attempt,
                delay,
                error,
            } => write!(
                f,
                "Attempt {attempt} failed ({error}); retrying in {}s…",
                delay.as_secs()
            ),
            Event::ArchiveKept { path } => {
                write!(f, "Kept for the next run: {}", path.display())
            }
            Event::ParsingArchive => write!(f, "Parsing XML manuscripts…"),
            Event::EntriesSkipped {
                by_path,
                by_content,
            } => write!(
                f,
                "Skipped {} non-manuscript entries ({by_path} by path, {by_content} by content).",
                by_path + by_content
            ),
            Event::Indexed { manuscripts } => write!(f, "Indexed {manuscripts} manuscripts."),
            Event::ReadingHeaders => write!(f, "Reading headers…"),
            Event::HeadersRead {
                manuscripts,
                groups,
            } => write!(f, "  {manuscripts} manuscripts in {groups} groups"),
            Event::WritingDocuments { documents } => write!(f, "Writing {documents} documents…"),
            Event::DocumentsWritten { done, total } => write!(f, "  {done} of {total} documents"),
            Event::CheckingPackage => write!(f, "Checking the package…"),
            Event::CheckingPublished => write!(f, "Checking the published copy…"),
            Event::PreviousPackageLeft { path } => write!(
                f,
                "  note: the previous package is left at {}",
                path.display()
            ),
        }
    }
}

/// Somewhere for a run to say what it is doing.
///
/// `Send + Sync` from the start, though nothing in this crate is threaded yet:
/// a sink that forwards to a window is shared by whatever thread the work ends
/// up on, and requiring it later would be a breaking change to every
/// implementation rather than to none.
///
/// `&self`, not `&mut self`, for the same reason — a sink two callers hold at
/// once cannot be borrowed mutably, and one that needs to accumulate can reach
/// for a `Mutex` and pay for it only where it is wanted.
pub trait Progress: Send + Sync {
    /// Take one event. Must not fail: a run that has its data is not failed by
    /// having nowhere to say so.
    fn report(&self, event: Event<'_>);
}

/// The sink the CLI runs with: one line on stderr per stage.
///
/// The wording is [`Event`]'s, so this is the whole of it — apart from the one
/// judgement a line-oriented sink has to make. Ticks are dropped: the terminal
/// gets the seventeen stage lines it has always got, not those plus fifty
/// fractions of the two stages that take time. What a tick is, and why it is
/// the event and not this sink that decides, is on [`Event::is_tick`].
pub struct Stderr;

impl Progress for Stderr {
    fn report(&self, event: Event<'_>) {
        if event.is_tick() {
            return;
        }
        eprintln!("{event}");
    }
}

/// The sink that hears everything and says nothing.
///
/// What the tests run with — a suite that printed the parse of every synthetic
/// archive buried the failures in progress — and what an embedder passes when
/// it wants the work without the commentary.
pub struct Silent;

impl Progress for Silent {
    fn report(&self, _event: Event<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// A sink that keeps what it was told, for tests that care about the order
    /// of the stages rather than their wording.
    #[derive(Default)]
    struct Recording(Mutex<Vec<String>>);

    impl Progress for Recording {
        fn report(&self, event: Event<'_>) {
            self.0.lock().expect("not poisoned").push(event.to_string());
        }
    }

    /// The seventeen lines the core used to print, as it printed them.
    ///
    /// This is the whole promise of the change: the sink moved, the output did
    /// not. Every string here was copied from the `eprintln!` it replaced, so a
    /// failure means the wording drifted rather than that the test is stale.
    #[test]
    fn the_wording_is_what_the_core_used_to_print() {
        let dir = PathBuf::from("/cache/aruna");
        let err = ArunaError::Truncated {
            url: "u".into(),
            expected: 10,
            got: 4,
        };
        let cases: [(Event<'_>, &str); 17] = [
            (
                Event::CacheUnusable { dir: &dir },
                "Cannot write to the cache directory (/cache/aruna); downloading for this run only.",
            ),
            (
                Event::CachedArchiveRejected,
                "Cached archive failed its checksum; downloading it again.",
            ),
            (
                Event::ArchiveFromCache { path: &dir },
                "Using the archive already downloaded: /cache/aruna",
            ),
            (
                Event::ZenodoNotice {
                    message: "A newer edition of the corpus is published",
                },
                "A newer edition of the corpus is published",
            ),
            (
                Event::ZenodoUnreachable { cause: "timed out" },
                "Could not check the record on Zenodo (timed out); continuing.",
            ),
            (
                Event::DownloadStarted,
                "Downloading TLHdig archive from Zenodo…",
            ),
            (
                Event::DownloadRetrying {
                    attempt: 2,
                    delay: Duration::from_secs(4),
                    error: &err,
                },
                "Attempt 2 failed (truncated download of u: expected 10 bytes, got 4); \
                 retrying in 4s…",
            ),
            (
                Event::ArchiveKept { path: &dir },
                "Kept for the next run: /cache/aruna",
            ),
            (Event::ParsingArchive, "Parsing XML manuscripts…"),
            (
                Event::EntriesSkipped {
                    by_path: 20,
                    by_content: 3,
                },
                "Skipped 23 non-manuscript entries (20 by path, 3 by content).",
            ),
            (
                Event::Indexed { manuscripts: 24_501 },
                "Indexed 24501 manuscripts.",
            ),
            (Event::ReadingHeaders, "Reading headers…"),
            (
                Event::HeadersRead {
                    manuscripts: 24_501,
                    groups: 799,
                },
                "  24501 manuscripts in 799 groups",
            ),
            (
                Event::WritingDocuments { documents: 24_501 },
                "Writing 24501 documents…",
            ),
            (Event::CheckingPackage, "Checking the package…"),
            (Event::CheckingPublished, "Checking the published copy…"),
            (
                Event::PreviousPackageLeft { path: &dir },
                "  note: the previous package is left at /cache/aruna",
            ),
        ];

        for (event, expected) in cases {
            // Seventeen stages, and the definition of a stage is that this sink
            // prints it: `Stderr` drops a tick, so a milestone that answered
            // true here would silently stop reaching the terminal.
            assert!(!event.is_tick(), "a stage was classified as a tick");
            assert_eq!(event.to_string(), expected);
        }
    }

    /// The two refinements, apart from the seventeen stages above.
    ///
    /// They were never printed, so they are not in that list — the wording here
    /// is for a sink that draws rather than prints, and the classification is
    /// what keeps them out of the terminal. `total` is optional on the transfer
    /// alone: a server that announces no length leaves a numerator with no
    /// denominator, and saying so is better than inventing one.
    #[test]
    fn a_tick_says_how_far_along_a_stage_is_and_is_not_one() {
        let cases: [(Event<'_>, &str); 3] = [
            (
                Event::Downloading {
                    bytes: 1_048_576,
                    total: Some(74_907_648),
                },
                "  1048576 of 74907648 bytes",
            ),
            (
                Event::Downloading {
                    bytes: 1_048_576,
                    total: None,
                },
                "  1048576 bytes",
            ),
            (
                Event::DocumentsWritten {
                    done: 500,
                    total: 23_936,
                },
                "  500 of 23936 documents",
            ),
        ];

        for (event, expected) in cases {
            assert!(event.is_tick(), "a tick was classified as a stage");
            assert_eq!(event.to_string(), expected);
        }
    }

    /// The count is derived, not carried: a sink that wants the two halves has
    /// them, and the line that shows a total cannot show one that disagrees.
    #[test]
    fn the_skipped_total_is_the_two_halves_added_up() {
        assert!(Event::EntriesSkipped {
            by_path: 7,
            by_content: 5,
        }
        .to_string()
        .starts_with("Skipped 12 "));
    }

    /// The point of the trait: the same event reaches a sink that keeps it and
    /// a sink that drops it, and neither one is the core's business.
    #[test]
    fn a_sink_hears_every_event_in_order() {
        let sink = Recording::default();
        let progress: &dyn Progress = &sink;
        progress.report(Event::ParsingArchive);
        progress.report(Event::Indexed { manuscripts: 3 });
        assert_eq!(
            *sink.0.lock().expect("not poisoned"),
            ["Parsing XML manuscripts…", "Indexed 3 manuscripts."]
        );
    }

    #[test]
    fn silence_is_a_sink_too() {
        let progress: &dyn Progress = &Silent;
        progress.report(Event::CheckingPackage);
    }
}
