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
    // The last path segment, and nothing before it. This used to split on
    // `/files/` — which the pinned Zenodo URL happens to contain — and a URL
    // without that segment fell through to the whole URL: `rsplit` on a
    // needle that is not there yields the haystack. The name then carried
    // `http://host/…` separators and all, so joining it to the cache
    // directory wrote the archive into a tree of directories named after the
    // URL, and a `..` in one would have written outside the cache entirely.
    let file = url
        .split('?')
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && *name != "." && *name != "..")
        .filter(|name| !name.contains(['/', '\\']))
        .unwrap_or("archive.zip");
    match file.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem}.{md5}.{ext}"),
        None => format!("{file}.{md5}"),
    }
}

/// What the cache had for `md5`.
///
/// Three outcomes rather than `Option`, because two of the misses are the same
/// to the caller and different to the reader: nothing under that name is the
/// ordinary cold run, and a file under that name that hashes to something else
/// is worth saying out loud. Returned rather than printed so this module needs
/// no opinion about who is listening — see [`crate::progress`].
pub enum Lookup {
    /// There, and still hashing to what its name promises.
    Hit(PathBuf),
    /// Nothing under that name.
    Absent,
    /// Something under that name, and not this archive.
    Rejected,
}

/// The cached archive for `md5`, if it is there and still hashes to it.
///
/// A file that fails the check is reported as a miss rather than deleted here:
/// the caller downloads over it, and the rename that lands the new copy
/// replaces it atomically.
pub fn lookup(dir: &Path, url: &str, md5: &str) -> Lookup {
    let path = dir.join(archive_name(url, md5));
    if !path.is_file() {
        return Lookup::Absent;
    }
    match digest_of(&path) {
        Ok(found) if found.eq_ignore_ascii_case(md5) => Lookup::Hit(path),
        Ok(_) => Lookup::Rejected,
        // Unreadable is not "the wrong archive": nothing is known about the
        // bytes, so there is nothing to tell the reader that the download
        // about to happen will not tell them better.
        Err(_) => Lookup::Absent,
    }
}

/// Delete the editions of this archive that `keep` has superseded.
///
/// Run after a download succeeds, so republishing the record does not leave
/// 71 MiB of superseded archive behind for good. Failures are ignored: a cache
/// that could not be tidied is not a reason to fail a run that has its data.
///
/// Only names this module produced are removed — the same file under a
/// different digest. It used to take every `*.zip` in the directory, which is
/// right for a cache of our own and destructive anywhere else: [`CACHE_DIR_ENV`]
/// is a setting a user can point wherever they like, and pointed at a directory
/// holding other archives it would have deleted them.
pub fn prune(dir: &Path, keep: &Path) {
    let Some(keep_name) = keep.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    // A name we cannot read as ours is a reason to remove nothing at all.
    let Some(current) = archive_parts(keep_name) else {
        return;
    };

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == keep_name {
            continue;
        }
        if archive_parts(name) == Some(current) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// The parts of a name [`archive_name`] built that identify the archive rather
/// than the edition: what it is called at the source, and its extension.
///
/// `None` for anything not of that shape, which is how a file that is not ours
/// is recognised.
fn archive_parts(name: &str) -> Option<(&str, &str)> {
    let (rest, ext) = name.rsplit_once('.')?;
    let (stem, digest) = rest.rsplit_once('.')?;
    let is_digest = digest.len() == 32 && digest.bytes().all(|b| b.is_ascii_hexdigit());
    (is_digest && !stem.is_empty()).then_some((stem, ext))
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

/// Whether `dir` can be created and written to.
///
/// Asked before the cache is relied on, because a cache that cannot be written
/// is a reason to skip caching — not a reason to fail a run that would
/// otherwise succeed. A directory can be unwritable for ordinary reasons: a
/// restricted account, a volume mounted read-only, a permissions repair gone
/// wrong.
pub fn is_usable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".aruna-write-test.{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Stream a file through MD5, as an [`ArunaError`] rather than an `io::Error`.
fn digest_of(path: &Path) -> Result<String> {
    crate::md5::md5_file(path).map_err(ArunaError::io(path))
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

    /// A cache file name is one path component, whatever the URL looks like.
    ///
    /// It was not: the name was taken by splitting on `/files/`, and a URL
    /// without that segment yielded the whole URL — scheme, host and all — so
    /// `dir.join(name)` built a tree of directories named after the URL inside
    /// the cache. A `..` among them would have written outside it. Found by
    /// `tests/cache_lifecycle.rs`, which is the first thing to point the
    /// download at a local server.
    #[test]
    fn the_name_is_always_a_single_path_component() {
        for url in [
            "https://zenodo.org/records/20328284/files/TLHbasis.zip?download=1",
            "http://127.0.0.1:8080/TLHbasis.zip",
            "http://example.invalid/a/b/c/corpus.zip",
            "http://example.invalid/../../etc/passwd",
            "http://example.invalid/",
            "not even a url",
            "",
        ] {
            let name = archive_name(url, "abc123");
            assert!(
                !name.contains('/') && !name.contains('\\'),
                "{url:?} produced a name with a separator in it: {name}"
            );
            assert!(!name.starts_with(".."), "{url:?} produced {name}");
            assert!(!name.is_empty());
            // And joining it stays inside the directory it was joined to.
            let joined = std::path::Path::new("/cache").join(&name);
            assert_eq!(
                joined.parent(),
                Some(std::path::Path::new("/cache")),
                "{url:?} escaped the cache directory as {name}"
            );
        }
    }

    /// The name a URL without `/files/` gets is the file it names, not the URL.
    #[test]
    fn a_plain_url_is_named_after_its_file() {
        assert_eq!(
            archive_name("http://127.0.0.1:8080/TLHbasis.zip", "abc123"),
            "TLHbasis.abc123.zip"
        );
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

        assert!(matches!(
            lookup(dir.path(), url, &md5),
            Lookup::Hit(path) if path == dir.path().join(archive_name(url, &md5))
        ));
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

        assert!(matches!(lookup(dir.path(), url, &md5), Lookup::Rejected));
    }

    #[test]
    fn an_empty_cache_is_a_miss() {
        let dir = tempdir().unwrap();
        let url = "https://zenodo.org/records/1/files/corpus.zip?download=1";
        assert!(matches!(
            lookup(dir.path(), url, "d41d8cd98f00b204e9800998ecf8427e"),
            Lookup::Absent
        ));
    }

    #[test]
    fn pruning_keeps_the_current_archive_and_nothing_else() {
        let dir = tempdir().unwrap();
        let url = "https://zenodo.org/records/1/files/corpus.zip?download=1";
        let keep = dir.path().join(archive_name(url, &md5_hex(b"new")));
        let old = dir.path().join(archive_name(url, &md5_hex(b"old")));
        let unrelated = dir.path().join("notes.txt");
        for path in [&keep, &old, &unrelated] {
            std::fs::write(path, b"x").unwrap();
        }

        prune(dir.path(), &keep);

        assert!(keep.is_file(), "the archive this run uses stays");
        assert!(!old.is_file(), "a superseded archive is 71 MiB of nothing");
        assert!(unrelated.is_file(), "only archives are ours to remove");
    }

    /// The cache can be sent anywhere by [`CACHE_DIR_ENV`], so "ours" has to
    /// mean the names this module builds — not every archive it happens to find
    /// beside its own. Pointed at a directory of someone's downloads, the old
    /// rule emptied it of zips.
    #[test]
    fn pruning_leaves_archives_that_are_not_ours() {
        let dir = tempdir().unwrap();
        let url = "https://zenodo.org/records/1/files/corpus.zip?download=1";
        let keep = dir.path().join(archive_name(url, &md5_hex(b"new")));
        let strangers = [
            "holiday-photos.zip",                          // no digest at all
            "corpus.zip",                                  // the same name, undigested
            "corpus.not-a-digest.zip",                     // a middle part that is not one
            "other.d41d8cd98f00b204e9800998ecf8427e.zip",  // digest-named, different archive
            "corpus.d41d8cd98f00b204e9800998ecf8427e.tar", // same archive, another format
        ];
        std::fs::write(&keep, b"x").unwrap();
        for name in strangers {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }

        prune(dir.path(), &keep);

        assert!(keep.is_file());
        for name in strangers {
            assert!(
                dir.path().join(name).is_file(),
                "{name} is not this cache's to delete"
            );
        }
    }

    /// A destination we cannot recognise as our own means removing nothing:
    /// the alternative is deciding what to delete from a name we do not
    /// understand.
    #[test]
    fn pruning_a_directory_we_did_not_name_removes_nothing() {
        let dir = tempdir().unwrap();
        let keep = dir.path().join("archive.zip");
        let other = dir
            .path()
            .join("corpus.d41d8cd98f00b204e9800998ecf8427e.zip");
        for path in [&keep, &other] {
            std::fs::write(path, b"x").unwrap();
        }

        prune(dir.path(), &keep);

        assert!(keep.is_file());
        assert!(other.is_file());
    }

    /// The override exists so a test — or a user short of disk — can send the
    /// cache somewhere else.
    #[test]
    fn the_environment_can_move_the_cache() {
        assert_eq!(CACHE_DIR_ENV, "ARUNA_CACHE_DIR");
    }

    /// A cache that can be written to is one worth using, and asking creates it
    /// — the caller relies on that and does not create it a second time.
    #[test]
    fn a_writable_cache_is_usable_and_is_created_by_the_asking() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("aruna");
        assert!(!cache.exists());

        assert!(is_usable(&cache));

        assert!(
            cache.is_dir(),
            "asking must leave the directory ready to use"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&cache)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(leftovers.is_empty(), "the probe left {leftovers:?} behind");
    }

    /// The question this function exists to answer. A cache that cannot be
    /// written is a reason to skip caching, and the answer has to be `false`
    /// rather than an error — the run continues without it.
    #[cfg(unix)]
    #[test]
    fn a_cache_that_cannot_be_written_is_not_usable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();

        // Cannot be created inside a directory that refuses writes…
        assert!(!is_usable(&locked.join("aruna")));
        // …and cannot be written to even where it already exists.
        assert!(!is_usable(&locked));

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
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

        assert!(
            !stale.is_file(),
            "an interrupted download from yesterday goes"
        );
        assert!(
            fresh.is_file(),
            "a download that may still be running stays"
        );
        assert!(archive.is_file(), "the archive itself is not a leftover");
    }
}
