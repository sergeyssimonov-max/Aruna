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

    /// A finished package does not match the model it was built from. The count
    /// is the whole tally; the text is the first few, because a package with
    /// four hundred broken links is one problem, not four hundred.
    #[error("{root} failed validation with {count} problem(s): {first}")]
    ExportInvalid {
        root: std::path::PathBuf,
        count: usize,
        first: String,
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

pub type Result<T> = std::result::Result<T, ArunaError>;
