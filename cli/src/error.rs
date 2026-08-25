//! Error types for Aruna.

use std::path::PathBuf;

/// Application-level error with enough context for the CLI and tests.
#[derive(Debug, thiserror::Error)]
pub enum ArunaError {
    #[error("network error while downloading {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("HTTP {status} while downloading {url}")]
    Http {
        url: String,
        status: u16,
        /// `Retry-After`, in seconds, when the server sent one. A 429 or 503
        /// usually carries it, and ignoring it is how a client gets throttled
        /// harder than it needs to be.
        retry_after: Option<u64>,
    },

    /// The server announced `expected` bytes and delivered fewer. Caught here
    /// rather than downstream, where a short ZIP surfaces as a confusing
    /// "invalid archive" long after the real failure.
    #[error("truncated download of {url}: expected {expected} bytes, got {got}")]
    Truncated {
        url: String,
        expected: u64,
        got: u64,
    },

    /// The body kept coming. `Truncated` catches a transfer that stopped early;
    /// this is the other end of the same question, and it is the one that costs
    /// something to find out late — bytes that are never going to be accepted
    /// are still bytes written to the user's disk while they arrive.
    #[error("oversized download of {url}: stopped after {got} bytes, limit is {limit}")]
    Oversized { url: String, limit: u64, got: u64 },

    /// The bytes arrived intact by length but hash to something else. Zenodo
    /// publishes an MD5 per file, so a silently corrupted or republished archive
    /// is caught here instead of surfacing as an obscure parse failure.
    #[error("checksum mismatch for {url}: expected MD5 {expected}, got {got}")]
    ChecksumMismatch {
        url: String,
        expected: String,
        got: String,
    },

    #[error("failed to read ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("ZIP archive is empty or contains no XML documents")]
    EmptyArchive,

    /// The caller asked the run to stop, and it did.
    ///
    /// An outcome, not a fault: nothing went wrong, somebody changed their
    /// mind. It travels as an error because that is how a stop propagates out
    /// of a loop nested five calls deep without every function in between
    /// growing a third return case — and because the cleanup a `?` triggers on
    /// the way out is exactly the cleanup a cancelled run needs. A `Drop` that
    /// removes a staging directory does not care why it is being dropped.
    ///
    /// Callers that show this to a person must not word it as a failure; see
    /// `ArunaError::is_cancellation`.
    #[error("cancelled during {phase}")]
    Cancelled { phase: crate::job::Phase },

    /// Every I/O failure carries the path it happened at — there is deliberately
    /// no bare `#[from] io::Error` variant, so a path can never be dropped.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The new inventory was written and flushed, but could not take the
    /// destination's place — on Windows a replace fails while the old file is
    /// open in another process (a browser showing the previous run), when it is
    /// read-only, or under a transient lock from a scanner. The finished file is
    /// kept and named here rather than discarded: it cost a download and a full
    /// parse, and it is complete.
    #[error("could not replace {path}; the new inventory is complete and kept at {scratch}")]
    Replace {
        path: PathBuf,
        scratch: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Two documents wanted the same place in the export, and neither may be
    /// the one that survives. The message names both so the pair can be looked
    /// at rather than guessed about.
    #[error("export collision in {group}: {fragment} maps to {path:?}, wanted by both {first} and {second}")]
    ExportCollision {
        group: String,
        fragment: String,
        first: String,
        second: String,
        path: std::path::PathBuf,
    },

    /// One archive entry is larger than the export will hold in memory. The
    /// archive is compressed, so a few hundred kilobytes on disk can be a few
    /// hundred megabytes once inflated — measured at 834 MiB of peak memory
    /// from a 398 KiB file before this limit existed.
    #[error("{entry} is larger than the {limit} byte limit for one document")]
    ExportDocumentTooLarge { entry: String, limit: u64 },

    /// A document came out of normalisation differing from its source in a way
    /// the permit list does not cover. The highest-priority guarantee of the
    /// export is that this cannot happen silently, so it stops the build rather
    /// than publishing the document or the package around it.
    #[error("{entry} was distorted by normalisation: {reason}")]
    ExportDistorted { entry: String, reason: String },

    /// The export wrote a different number of documents than it placed. Not a
    /// condition any input should produce — it means the writer and the
    /// placement disagreed about what the archive holds — so it is reported
    /// rather than absorbed.
    #[error("export wrote {written} documents but placed {expected}")]
    ExportIncomplete { expected: usize, written: usize },

    /// An archive with more entries than a corpus has documents.
    ///
    /// The per-document ceiling bounds one entry; nothing bounded how many
    /// there were. An archive of a million empty entries costs a name, a header
    /// parse and a gate decision each — no inflation, and so nothing the size
    /// limit would notice — and the real corpus is 24 537 entries, so the shape
    /// of that attack is two orders of magnitude away from anything real.
    #[error("archive holds {entries} entries, more than the {limit} this program will read")]
    ArchiveTooManyEntries { entries: usize, limit: usize },

    /// A package that would be larger than any corpus this program is for.
    ///
    /// The companion to [`Self::ExportDocumentTooLarge`]: that one bounds a
    /// document, this one bounds their sum. An archive of many documents each
    /// just under the per-document limit passed both the entry count and the
    /// size check and still filled a disk.
    #[error("the package reached {written} bytes, more than the {limit} this program will write")]
    ExportPackageTooLarge { written: u64, limit: u64 },

    /// A finished package does not match the model it was built from. The count
    /// is the whole tally; the text is the first few, because a package with
    /// four hundred broken links is one problem, not four hundred.
    #[error("{root} failed validation with {count} problem(s): {first}")]
    ExportInvalid {
        root: std::path::PathBuf,
        count: usize,
        first: String,
    },

    /// Another run is publishing into the same directory and has not finished.
    ///
    /// Publishing is a move, a rename and a read-back, and two runs interleaving
    /// there leave one of them checking a package the other replaced. They take
    /// turns instead; this is the run that waited long enough to conclude the
    /// lock is not going to be released.
    #[error("another run is publishing into {path} ({holder})")]
    PublishBusy {
        path: std::path::PathBuf,
        holder: String,
    },

    /// The destination holds something this exporter did not write, and a
    /// recursive delete aimed at the wrong directory cannot be taken back.
    #[error("refusing to replace {path}: {reason}")]
    ExportDestination {
        path: std::path::PathBuf,
        reason: String,
    },

    #[error("could not resolve Downloads directory")]
    DownloadsDir,
}

impl ArunaError {
    /// The `map_err` argument for an I/O call that happened at `path`.
    ///
    /// [`Self::Io`] deliberately has no `#[from] io::Error`, so that a path can
    /// never be dropped on the way out — and the cost of that decision was the
    /// same four lines written twenty-three times across eight modules:
    ///
    /// ```text
    /// .map_err(|source| ArunaError::Io { path: p.to_path_buf(), source })?
    /// ```
    ///
    /// This is that, said once: `.map_err(ArunaError::io(&p))?`. `write_atomic`
    /// had already grown its own local version of it, which is what a thing
    /// being needed in more than one place looks like before it is moved.
    ///
    /// `impl Into<PathBuf>` takes every shape the call sites pass — `&Path`,
    /// `&PathBuf`, `PathBuf`, `&str`, `String` — so the conversion is written
    /// here rather than at each site. It returns `FnOnce` because it makes a
    /// fresh closure per call: a site inside a loop mints one each time round,
    /// and one that also needs the path afterwards passes a reference.
    pub fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> ArunaError {
        let path = path.into();
        move |source| ArunaError::Io { path, source }
    }
}

pub type Result<T> = std::result::Result<T, ArunaError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Every error must name what it is about.
    ///
    /// Across 23 936 documents an error that says only what went wrong and not
    /// what it went wrong on is not actionable, and the next stage — a batch of
    /// the same size producing PDFs — will make that worse rather than better.
    /// Listed one by one rather than derived, so adding a variant means coming
    /// here and saying what identifies it.
    #[test]
    fn no_error_is_anonymous() {
        let io = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let cases: Vec<(ArunaError, &str)> = vec![
            (
                ArunaError::Http {
                    status: 503,
                    url: "https://example.invalid/a.zip".into(),
                    retry_after: None,
                },
                "a.zip",
            ),
            (
                ArunaError::Truncated {
                    url: "https://example.invalid/b.zip".into(),
                    expected: 10,
                    got: 3,
                },
                "b.zip",
            ),
            (
                ArunaError::Oversized {
                    url: "https://example.invalid/c.zip".into(),
                    limit: 10,
                    got: 11,
                },
                "c.zip",
            ),
            (
                ArunaError::ChecksumMismatch {
                    url: "https://example.invalid/d.zip".into(),
                    expected: "aa".into(),
                    got: "bb".into(),
                },
                "d.zip",
            ),
            (
                ArunaError::Io {
                    path: PathBuf::from("/tmp/e.xml"),
                    source: io(),
                },
                "e.xml",
            ),
            (
                ArunaError::Replace {
                    path: PathBuf::from("/tmp/f.html"),
                    scratch: PathBuf::from("/tmp/f.tmp"),
                    source: io(),
                },
                "f.html",
            ),
            (
                ArunaError::ExportCollision {
                    group: "CTH 5".into(),
                    fragment: "KBo 1.1".into(),
                    first: "a.xml".into(),
                    second: "b.xml".into(),
                    path: PathBuf::from("CTH 5/KBo 1.1.xml"),
                },
                "KBo 1.1",
            ),
            (
                ArunaError::ExportDocumentTooLarge {
                    entry: "g.xml".into(),
                    limit: 1,
                },
                "g.xml",
            ),
            (
                ArunaError::ExportDistorted {
                    entry: "h.xml".into(),
                    reason: "why".into(),
                },
                "h.xml",
            ),
            (
                ArunaError::ExportInvalid {
                    root: PathBuf::from("/tmp/pkg"),
                    count: 2,
                    first: "a link points at nothing".into(),
                },
                "/tmp/pkg",
            ),
            (
                ArunaError::ExportDestination {
                    path: PathBuf::from("/tmp/theirs"),
                    reason: "it is not ours".into(),
                },
                "/tmp/theirs",
            ),
        ];

        for (error, subject) in cases {
            let text = error.to_string();
            assert!(
                text.contains(subject),
                "{text:?} does not say which {subject} it is about"
            );
        }
    }

    /// The three that identify no subject, and why that is right.
    ///
    /// `EmptyArchive` and `DownloadsDir` are about the one archive and the one
    /// folder the run already named; `ExportIncomplete` is a disagreement
    /// between two counts and the counts are the subject. Pinned so that a new
    /// variant cannot join them by accident.
    #[test]
    fn the_errors_without_a_subject_are_only_the_ones_that_cannot_have_one() {
        assert_eq!(
            ArunaError::EmptyArchive.to_string(),
            "ZIP archive is empty or contains no XML documents"
        );
        assert_eq!(
            ArunaError::DownloadsDir.to_string(),
            "could not resolve Downloads directory"
        );
        let counts = ArunaError::ExportIncomplete {
            expected: 5,
            written: 4,
        }
        .to_string();
        assert!(counts.contains('5') && counts.contains('4'), "{counts}");
    }
}
