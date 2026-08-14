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

    #[error("could not resolve Downloads directory")]
    DownloadsDir,
}

pub type Result<T> = std::result::Result<T, ArunaError>;
