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
    Http { url: String, status: u16 },

    /// The server announced `expected` bytes and delivered fewer. Caught here
    /// rather than downstream, where a short ZIP surfaces as a confusing
    /// "invalid archive" long after the real failure.
    #[error("truncated download of {url}: expected {expected} bytes, got {got}")]
    Truncated {
        url: String,
        expected: u64,
        got: u64,
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

    #[error("could not resolve Downloads directory")]
    DownloadsDir,
}

pub type Result<T> = std::result::Result<T, ArunaError>;
