//! Output path resolution (`~/Downloads/...`) and atomic file replacement.

use crate::error::{ArunaError, Result};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

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

/// Sibling scratch path used by [`write_atomic`].
///
/// The process id keeps two concurrent runs from writing the same scratch file,
/// and keeping it next to the destination keeps the later rename on a single
/// filesystem — across devices `rename` fails instead of being atomic.
fn scratch_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.part", std::process::id()));
    path.with_file_name(name)
}

/// Replace `path` with `bytes`, atomically.
///
/// Writes a sibling scratch file, forces it to disk, then renames it over the
/// destination. `fs::write` would truncate the destination first, so a crash, a
/// full disk or a killed process would leave the user with a half-written
/// inventory in place of the working one from the previous run. Rename makes
/// the swap all-or-nothing: readers see either the old file or the new one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let scratch = scratch_sibling(path);
    let io_err = |p: &Path| {
        let p = p.to_path_buf();
        move |source| ArunaError::Io { path: p, source }
    };

    let mut file = File::create(&scratch).map_err(io_err(&scratch))?;
    // Any failure past this point leaves the scratch file behind; drop it so a
    // failed run does not litter the output directory.
    let written = file
        .write_all(bytes)
        // `sync_all` matters before the rename: without it the rename can reach
        // the disk ahead of the contents, and a power loss then leaves an empty
        // file where a valid inventory used to be.
        .and_then(|()| file.sync_all());
    if let Err(source) = written {
        drop(file);
        let _ = std::fs::remove_file(&scratch);
        return Err(ArunaError::Io {
            path: scratch,
            source,
        });
    }
    drop(file);

    std::fs::rename(&scratch, path).map_err(|source| {
        let _ = std::fs::remove_file(&scratch);
        ArunaError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
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
    fn write_atomic_creates_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.html");

        write_atomic(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");

        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
    }

    /// The scratch file must not survive a successful write — otherwise every
    /// run would litter the user's Downloads folder.
    #[test]
    fn write_atomic_leaves_no_scratch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.html");
        write_atomic(&path, b"body").unwrap();

        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names.len(), 1, "unexpected leftovers: {names:?}");
    }

    /// The scratch path stays in the destination directory: a rename across
    /// filesystems is not atomic and on many platforms fails outright.
    #[test]
    fn scratch_stays_next_to_destination() {
        let path = std::path::Path::new("/tmp/aruna/out.html");
        let scratch = scratch_sibling(path);
        assert_eq!(scratch.parent(), path.parent());
        assert!(scratch
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("out.html.") && n.ends_with(".part")));
    }

    #[test]
    fn ensure_parent_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("out.html");
        ensure_output_parent(&path).unwrap();
        assert!(path.parent().unwrap().is_dir());
    }
}
