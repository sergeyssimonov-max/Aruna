//! Output path resolution (`~/Downloads/...`) and atomic file replacement.

use crate::error::{ArunaError, Result};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Canonical output file name.
///
/// Names the edition the inventory was built from rather than the work it
/// catalogues: the title inside the document stays *Thesaurus Linguarum
/// Hethaeorum Digitalis*, while the file says which release of TLHdig produced
/// it — which is what tells two downloads apart in a Downloads folder.
///
/// No spaces, so it survives a shell, a URL and an email attachment unquoted.
///
/// **The one place the inventory is named.** The exporter used to write
/// `format!("{PACKAGE}.html")` in four places while the window and the
/// standalone path read this constant; the two agreed only because
/// `PACKAGE` happens to be `TLHdig_Beta_0.3`, and nothing said they had to.
/// The day the corpus edition moves to Beta 0.4, that arrangement would have
/// renamed the file the exporter writes and left the window reporting
/// `inventory_exists: false` about a package that was right there. Everything
/// now joins this constant, and
/// [`the_inventory_is_named_after_the_package`] holds the relationship the
/// name still carries.
pub const OUTPUT_FILE_NAME: &str = "TLHdig_Beta_0.3.html";

/// Resolve `~/Downloads/TLHdig_Beta_0.3.html`.
pub fn output_html_path() -> Result<PathBuf> {
    Ok(downloads_dir()?.join(OUTPUT_FILE_NAME))
}

/// Where the program writes: the reader's Downloads folder.
///
/// The one place that decides it. `HOME` moves it, which is what makes the
/// tests hermetic — they set `HOME` to a temporary directory and everything
/// this program writes goes there with it.
pub fn downloads_dir() -> Result<PathBuf> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .ok_or(ArunaError::DownloadsDir)
}

/// Sibling scratch path: `<name>.<pid>.<n>.part`, beside `path`.
///
/// Two things have to be unique here and the process id only covered one of
/// them. Between processes it is what keeps two runs from writing the same
/// scratch file, and that was always true. Within one process it is not: every
/// caller got the same name back, so two downloads of one archive on two
/// threads shared a scratch file — the one that renamed second found nothing
/// left to rename and failed with `NotFound` on a path it had just written.
///
/// The quieter half is worse. The digest is checked on the scratch file and the
/// rename happens after, so with a shared name the bytes that were verified and
/// the bytes that were moved into place need not be the same ones. Nothing
/// downstream would say so; only [`crate::cache::lookup`] rehashing the file on
/// the next run would catch it, a run later and as a mysterious miss.
///
/// The CLI is one download per process and never met this. The crate is a
/// library, `download_verified` and `obtain_archive` are public, and the
/// desktop application this is written towards is expected to convert on
/// background threads — so the guarantee has to hold per call, not per process.
/// A counter costs one atomic increment and makes the name unique for the life
/// of the process, which is as long as any scratch file lives.
///
/// Keeping it next to the destination keeps the later rename on a single
/// filesystem — across devices `rename` fails instead of being atomic.
///
/// Both things this program writes are written this way — the inventory here
/// and the archive in [`crate::download`] — so the convention is stated once
/// and they cannot end up with different ideas of what a half-written file
/// looks like.
pub fn scratch_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.part", run_tag()));
    path.with_file_name(name)
}

/// What makes a name this run's own: `{process id}.{counter}`.
///
/// Two things this program writes need a name nobody else will choose — the
/// scratch file above, and the staging directory in
/// [`crate::export::build`] — and each grew its own copy of this, down to the
/// same `AtomicU64` and the same doc comment about `Relaxed`; the second one's
/// notes say outright that it is "the same shape as `scratch_sibling`". Two
/// copies of a uniqueness rule is one edit away from two different rules, and
/// the failures that follow are the quiet kind: a name that collides is only
/// noticed by whichever run loses.
///
/// One counter serves both. It is shared rather than per-caller because what it
/// has to guarantee is distinctness, and a counter that never repeats gives
/// that whoever asks; the numbers a given caller sees are simply not
/// consecutive, and nothing has ever read them.
///
/// `Relaxed` is enough: `fetch_add` is atomic whatever the ordering, and
/// nothing here depends on the counter ordering against other memory.
pub fn run_tag() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    format!(
        "{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
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

    let mut file = File::create(&scratch).map_err(ArunaError::io(&scratch))?;
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

    replace_with_retries(&scratch, path)
}

/// Number of attempts at the final replace, and the pause between them.
///
/// A replace can fail for reasons that pass: on Windows an indexer or a virus
/// scanner opens a freshly written file for a moment, and `rename` onto a file
/// held open by another process fails outright. A short retry covers that
/// without making a genuine failure slow to report.
const REPLACE_ATTEMPTS: u32 = 3;
const REPLACE_BACKOFF: std::time::Duration = std::time::Duration::from_millis(120);

fn replace_with_retries(scratch: &Path, path: &Path) -> Result<()> {
    let mut last = match std::fs::rename(scratch, path) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };
    for attempt in 1..REPLACE_ATTEMPTS {
        std::thread::sleep(REPLACE_BACKOFF * attempt);
        match std::fs::rename(scratch, path) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }

    // The scratch file survives on purpose. It holds the finished, flushed
    // inventory — deleting it would throw away a full download and parse to
    // leave the user with nothing but the previous run's output.
    Err(ArunaError::Replace {
        path: path.to_path_buf(),
        scratch: scratch.to_path_buf(),
        source: last,
    })
}

/// Fail now if the inventory could not be written when it is ready.
///
/// The write is the last step of a run that costs a download and a full parse,
/// and a destination that cannot be written to is knowable at the start: an
/// account without permission, a read-only volume, a Downloads folder replaced
/// by something else. Reporting it a minute later, after the work, is the same
/// error delivered as late as possible.
///
/// The probe writes and removes a scratch file rather than reading permission
/// bits, which are not the whole answer on macOS — sandboxing and ACLs decide
/// too, and only an attempt reflects them.
pub fn check_output_writable(path: &Path) -> Result<()> {
    ensure_output_parent(path)?;
    let probe = scratch_sibling(path);
    std::fs::write(&probe, b"").map_err(ArunaError::io(&probe))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Ensure the parent Downloads directory exists.
pub fn ensure_output_parent(path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ArunaError::io(&parent))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// The README tells the reader where the inventory lands, by hand.
    ///
    /// It is the only statement of the path a person reads before running the
    /// tool, and renaming the output is exactly the change that leaves it
    /// pointing at a file that will never appear again. Included at compile
    /// time, so a moved README breaks the build rather than emptying the check.
    #[test]
    fn the_readme_names_the_file_the_tool_writes() {
        let readme = include_str!("../README.md");
        assert!(
            readme.contains(OUTPUT_FILE_NAME),
            "cli/README.md does not mention {OUTPUT_FILE_NAME} — it still documents an older name"
        );
    }

    /// The name has to survive being pasted somewhere without quoting.
    #[test]
    fn the_output_name_needs_no_quoting() {
        assert!(
            !OUTPUT_FILE_NAME.contains(' '),
            "a space here means every shell command and URL carrying this name has to quote it"
        );
        assert!(OUTPUT_FILE_NAME.ends_with(".html"));
    }

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

    /// The point of the check is that it costs nothing and leaves nothing.
    /// **The inventory is named after the package, and that is now checked.**
    ///
    /// Two constants said the same thing in different words —
    /// [`OUTPUT_FILE_NAME`] here and `format!("{PACKAGE}.html")` in the
    /// exporter — and agreed by coincidence. This is the coincidence written
    /// down: rename the package without renaming the file and the failure is
    /// this line, not a window that says the inventory is missing.
    #[test]
    fn the_inventory_is_named_after_the_package() {
        assert_eq!(
            OUTPUT_FILE_NAME,
            format!("{}.html", crate::export::PACKAGE),
            "the inventory's name and the package's name have drifted apart"
        );
    }

    #[test]
    fn the_writability_probe_leaves_the_directory_as_it_found_it() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("inventory").join(OUTPUT_FILE_NAME);

        check_output_writable(&out).expect("a fresh temp directory is writable");

        assert!(
            out.parent().unwrap().is_dir(),
            "the parent is created, as the write needs"
        );
        assert!(
            !out.exists(),
            "the inventory itself is not created by a probe"
        );
        let leftovers: Vec<_> = std::fs::read_dir(out.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(leftovers.is_empty(), "the probe left {leftovers:?} behind");
    }

    /// A destination that will refuse the inventory refuses the probe, which is
    /// the whole point: the run stops in half a second rather than after a
    /// download and a full parse.
    #[cfg(unix)]
    #[test]
    fn an_unwritable_destination_is_refused_up_front() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();

        let err = check_output_writable(&locked.join(OUTPUT_FILE_NAME))
            .expect_err("a read-only directory cannot take the inventory");
        assert!(
            matches!(err, ArunaError::Io { .. }),
            "the reader is told which path refused them: {err}"
        );

        // Leave it removable for the temp dir's own cleanup.
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
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

    /// Two callers wanting a scratch file for one destination are given two
    /// files.
    ///
    /// The name used to carry the process id and nothing else, which is unique
    /// between processes and identical within one. Two downloads of the same
    /// archive on two threads then shared a scratch path: the second rename
    /// found nothing to move, and — quieter — the bytes that passed the digest
    /// check need not have been the bytes that reached the destination.
    ///
    /// `tests/cache_concurrency.rs` reproduces that through six real downloads
    /// and takes eleven seconds to do it. This states the same invariant in a
    /// millisecond, so the fix has a test that runs on every change rather than
    /// only in the slow set.
    #[test]
    fn every_scratch_file_has_a_name_of_its_own() {
        let path = std::path::Path::new("/tmp/aruna/out.html");

        let sequential: std::collections::HashSet<_> =
            (0..64).map(|_| scratch_sibling(path)).collect();
        assert_eq!(sequential.len(), 64, "one process reused a scratch name");

        let concurrent: std::collections::HashSet<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| (0..32).map(|_| scratch_sibling(path)).collect::<Vec<_>>()))
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().expect("no thread panicked"))
                .collect()
        });
        assert_eq!(
            concurrent.len(),
            8 * 32,
            "two threads were handed the same scratch path"
        );
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

    /// A failed replace must not cost the user the run. The scratch file holds
    /// a complete, flushed inventory; the old code deleted it and left only the
    /// previous run's output behind.
    #[test]
    fn a_failed_replace_keeps_the_finished_inventory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("inventory.html");
        // A directory where the file should go makes `rename` fail on every
        // platform, which is the only portable way to reach this branch.
        std::fs::create_dir(&path).unwrap();

        let err = write_atomic(&path, b"finished inventory").unwrap_err();
        match err {
            ArunaError::Replace {
                scratch,
                path: reported,
                ..
            } => {
                assert_eq!(reported, path);
                assert_eq!(
                    std::fs::read(&scratch).unwrap(),
                    b"finished inventory",
                    "the completed inventory must survive a failed replace"
                );
                assert_eq!(scratch.parent(), path.parent());
            }
            other => panic!("expected a Replace error, got {other:?}"),
        }
    }

    /// The message has to name the kept file — it is the only way the user can
    /// find their inventory.
    #[test]
    fn failed_replace_message_names_both_paths() {
        let err = ArunaError::Replace {
            path: PathBuf::from("/tmp/out.html"),
            scratch: PathBuf::from("/tmp/out.html.42.part"),
            source: std::io::Error::other("busy"),
        };
        let text = err.to_string();
        assert!(text.contains("/tmp/out.html"), "{text}");
        assert!(text.contains("/tmp/out.html.42.part"), "{text}");
    }

    #[test]
    fn ensure_parent_creates_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("out.html");
        ensure_output_parent(&path).unwrap();
        assert!(path.parent().unwrap().is_dir());
    }
}
