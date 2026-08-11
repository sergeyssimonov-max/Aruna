//! Output path resolution (`~/Downloads/...`).

use crate::error::{ArunaError, Result};
use std::path::PathBuf;

/// Canonical output file name (with spaces, as specified).
pub const OUTPUT_FILE_NAME: &str = "Thesaurus Linguarum Hethaeorum Digitalis.html";

/// Resolve `~/Downloads/Thesaurus Linguarum Hethaeorum Digitalis.html`.
pub fn output_html_path() -> Result<PathBuf> {
    let downloads = dirs::download_dir().or_else(|| {
        dirs::home_dir().map(|h| h.join("Downloads"))
    });
    let dir = downloads.ok_or(ArunaError::DownloadsDir)?;
    Ok(dir.join(OUTPUT_FILE_NAME))
}

/// Ensure the parent Downloads directory exists.
pub fn ensure_output_parent(path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ArunaError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_path_ends_with_expected_name() {
        // On CI / containers download_dir or home/Downloads is usually available
        match output_html_path() {
            Ok(p) => {
                assert!(p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n == OUTPUT_FILE_NAME));
                assert!(p.is_absolute() || p.components().count() >= 2);
            }
            Err(ArunaError::DownloadsDir) => {
                // Extremely constrained environments without HOME
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[test]
    fn ensure_parent_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("out.html");
        ensure_output_parent(&path).unwrap();
        assert!(path.parent().unwrap().is_dir());
    }
}
