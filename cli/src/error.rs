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

    #[error("failed to read ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("ZIP archive is empty or contains no XML documents")]
    EmptyArchive,

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error: {0}")]
    IoSimple(#[from] std::io::Error),

    #[error("could not resolve Downloads directory")]
    DownloadsDir,

    #[error("invalid UTF-8 in archive entry {0}")]
    Utf8(String),
}

pub type Result<T> = std::result::Result<T, ArunaError>;
