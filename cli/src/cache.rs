//! Keeping the downloaded archive between runs.
//!
//! The archive is 71 MiB and changes only when Zenodo republishes the record —
//! which has not happened in the life of this program. Fetching it on every run
//! was the most expensive thing the tool did, by two orders of magnitude: about
//! a minute of network against 1.7 s of parsing, thrown away at the end of the
//! run and paid again on the next one.
//!
//! The cached file is named after the digest it is expected to have, so a
//! republished archive cannot be served out of the cache: a new digest is a new
//! name, and the old file is pruned when the new one lands. The digest is also
//! recomputed on every hit — 267 ms for 71 MiB, against the minute it saves —
//! because a file that has sat in a user-writable directory for months is not
//! something to take on trust.

use crate::error::{ArunaError, Result};
use crate::md5::Md5;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the archive came from, and whether it is ours to delete.
pub enum Archive {
    /// Kept for the next run.
    Cached(PathBuf),
    /// This run's own copy, in a scratch directory the caller cleans up.
    Temporary(PathBuf),
}

impl Archive {
    pub fn path(&self) -> &Path {
        match self {
            Archive::Cached(p) | Archive::Temporary(p) => p,
        }
    }
}

/// Overrides the cache location. Set it to a temporary directory to keep a run
/// from touching the real one.
pub const CACHE_DIR_ENV: &str = "ARUNA_CACHE_DIR";

/// Where archives are kept, or `None` when the platform offers nowhere to keep
/// them — in which case the tool still works, it just pays the download again.
///
/// `~/Library/Caches/aruna` on macOS, `~/.cache/aruna` elsewhere, and that is
/// the right place even though a cleaning tool will empty it: the file is
/// re-downloadable, and a directory the user can clear without losing anything
/// is exactly what a cache is. CleanMyMac and its like do delete it, which
/// costs the next run a minute and nothing else — verified by removing the
/// directory between runs.
///
/// So do not move this to Application Support to make it survive. That was
/// considered and declined: it would hide 71 MiB from the tools people use to
/// find 71 MiB.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(CACHE_DIR_ENV) {
        return Some(PathBuf::from(dir));
    }
    dirs::cache_dir().map(|dir| dir.join("aruna"))
}

/// File name for an archive: what it is called at the source, plus the digest
/// it must have.
///
/// The digest is in the name rather than only in a sidecar file so that the
/// question "is this the archive I want?" can be answered by the name, and a
/// republished archive lands beside the old one instead of over it.
pub fn archive_name(url: &str, md5: &str) -> String {
    let file = url
        .rsplit("/files/")
        .next()
        .and_then(|rest| rest.split('?').next())
        .filter(|name| !name.is_empty())
        .unwrap_or("archive.zip");
    match file.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.{md5}.{ext}"),
        None => format!("{file}.{md5}"),
    }
}

/// The cached archive for `md5`, if it is there and still hashes to it.
///
/// A file that fails the check is reported as a miss rather than deleted here:
/// the caller downloads over it, and the rename that lands the new copy
/// replaces it atomically.
pub fn lookup(dir: &Path, url: &str, md5: &str) -> Option<PathBuf> {
    let path = dir.join(archive_name(url, md5));
    if !path.is_file() {
        return None;
    }
    match digest_of(&path) {
        Ok(found) if found.eq_ignore_ascii_case(md5) => Some(path),
        Ok(_) => {
            eprintln!("Cached archive failed its checksum; downloading it again.");
            None
        }
        Err(_) => None,
    }
}

/// Delete every archive in `dir` except `keep`.
///
/// Run after a download succeeds, so republishing the record does not leave
/// 71 MiB of superseded archive behind for good. Failures are ignored: a cache
/// that could not be tidied is not a reason to fail a run that has its data.
pub fn prune(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "zip") {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// How old an unfinished download must be before it counts as abandoned.
///
/// [`crate::download::Scratch`] removes its own `.part` file when it goes out
/// of scope, which covers every error — but not a process that is killed, and
/// the leftovers used to land in a per-process temp directory the system swept
/// up. In a cache directory they would sit there for good: 10 MiB of nothing
/// for every interrupted run.
///
/// A day is long enough that no download in flight is ever mistaken for
/// abandoned, and short enough that the space comes back.
const ABANDONED_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Remove unfinished downloads that nothing is writing any more.
pub fn sweep_unfinished(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "part") {
            continue;
        }
        let abandoned = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .and_then(|at| at.elapsed().map_err(std::io::Error::other))
            .is_ok_and(|age| age >= ABANDONED_AFTER);
        if abandoned {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Stream a file through MD5.
fn digest_of(path: &Path) -> Result<String> {
    let io = |source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    };
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut digest = Md5::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf).map_err(io)?;
        if read == 0 {
            break;
        }
        digest.update(&buf[..read]);
    }
    Ok(digest.finish_hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md5::md5_hex;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn the_name_carries_the_archive_and_its_digest() {
        let name = archive_name(
            "https://zenodo.org/records/20328284/files/TLHbasis_Beta_03.zip?download=1",
            "abc123",
        );
        assert_eq!(name, "TLHbasis_Beta_03.abc123.zip");
    }

    /// Two releases of the archive are two files, so a republished record can
    /// never be answered out of the cache by its predecessor.
    #[test]
    fn a_different_digest_is_a_different_file() {
        let url = "https://zenodo.org/records/1/files/a.zip?download=1";
        assert_ne!(archive_name(url, "aaa"), archive_name(url, "bbb"));
    }

    #[test]
    fn a_file_that_hashes_as_promised_is_a_hit() {
        let dir = tempdir().unwrap();
        let url = "https://zenodo.org/records/1/files/corpus.zip?download=1";
        let body = b"pretend this is 71 MiB";
        let md5 = md5_hex(body);
        std::fs::write(dir.path().join(archive_name(url, &md5)), body).unwrap();

        assert_eq!(
            lookup(dir.path(), url, &md5),
            Some(dir.path().join(archive_name(url, &md5)))
        );
    }

    /// The name says what the contents must be, so contents that say otherwise
    /// are a miss — the point of checking is that this directory is writable by
    /// anyone, for months at a time.
    #[test]
    fn a_file_under_the_right_name_with_the_wrong_bytes_is_a_miss() {
        let dir = tempdir().unwrap();
        let url = "https://zenodo.org/records/1/files/corpus.zip?download=1";
        let md5 = md5_hex(b"the real archive");
        std::fs::write(dir.path().join(archive_name(url, &md5)), b"something else").unwrap();

        assert_eq!(lookup(dir.path(), url, &md5), None);
    }

    #[test]
    fn an_empty_cache_is_a_miss() {
        let dir = tempdir().unwrap();
        let url = "https://zenodo.org/records/1/files/corpus.zip?download=1";
        assert_eq!(lookup(dir.path(), url, "d41d8cd98f00b204e9800998ecf8427e"), None);
    }

    #[test]
    fn pruning_keeps_the_current_archive_and_nothing_else() {
        let dir = tempdir().unwrap();
        let keep = dir.path().join("corpus.new.zip");
        let old = dir.path().join("corpus.old.zip");
        let unrelated = dir.path().join("notes.txt");
        for path in [&keep, &old, &unrelated] {
            std::fs::write(path, b"x").unwrap();
        }

        prune(dir.path(), &keep);

        assert!(keep.is_file(), "the archive this run uses stays");
        assert!(!old.is_file(), "a superseded archive is 71 MiB of nothing");
        assert!(unrelated.is_file(), "only archives are ours to remove");
    }

    /// The override exists so a test — or a user short of disk — can send the
    /// cache somewhere else.
    #[test]
    fn the_environment_can_move_the_cache() {
        assert_eq!(CACHE_DIR_ENV, "ARUNA_CACHE_DIR");
    }

    /// A killed process leaves its `.part` behind — `Drop` does not run for a
    /// signal. In a directory that outlives the run, that is 10 MiB per
    /// interruption, kept for good.
    #[test]
    fn an_abandoned_download_is_swept_and_a_live_one_is_not() {
        let dir = tempdir().unwrap();
        let stale = dir.path().join("corpus.zip.111.part");
        let fresh = dir.path().join("corpus.zip.222.part");
        let archive = dir.path().join("corpus.abc.zip");
        for path in [&stale, &fresh, &archive] {
            std::fs::write(path, b"x").unwrap();
        }
        // Backdate the first one past the threshold.
        let old = std::time::SystemTime::now() - (ABANDONED_AFTER + Duration::from_secs(60));
        std::fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();

        sweep_unfinished(dir.path());

        assert!(!stale.is_file(), "an interrupted download from yesterday goes");
        assert!(fresh.is_file(), "a download that may still be running stays");
        assert!(archive.is_file(), "the archive itself is not a leftover");
    }
}
