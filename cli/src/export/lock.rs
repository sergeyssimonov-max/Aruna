//! One publication at a time, in one directory.
//!
//! Everything before publishing is already safe for two runs at once: each
//! stages under a name carrying its own process id, so neither can touch the
//! other's work. Publishing is the part that is not, and it was measured rather
//! than reasoned about — two runs on the real corpus left the destination
//! correct every time, and the losing run reported a validation failure.
//!
//! The reason is that publishing is three steps, not one: the existing package
//! is moved aside, the new one takes its name, and the copy that has just been
//! published is read back and checked. If the second run publishes in the gap
//! between the first run's rename and its read-back, the first run validates a
//! directory the second one replaced — and reports, correctly and uselessly,
//! that what it published is not what it built.
//!
//! So the three steps are made one: a lock file beside the package, held from
//! the moment the old package is moved until the new one has been checked. It
//! is advisory and cooperative — nothing stops another program writing there —
//! which is the right strength for what it defends against: this program, run
//! twice, by one person, on one machine.

use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::PACKAGE;

/// How long a lock may exist before it is assumed abandoned.
///
/// Nothing here can ask whether a process is alive: that is `kill(pid, 0)`, and
/// this crate forbids `unsafe`. Age is what is left, and it is enough because
/// the interval it bounds is short and known — moving a directory, renaming
/// another onto its name, and reading back 23 936 files, measured at two to six
/// seconds on the real corpus. Five minutes is fifty times that.
///
/// The cost of the choice, stated rather than hidden: a publication that
/// genuinely takes longer than this — a filesystem stalled, a corpus an order of
/// magnitude larger — can have its lock taken by a run that has been waiting.
/// Both then publish, and the package is still whichever finished last; what is
/// lost is the guarantee, not the data.
const STALE: Duration = Duration::from_secs(300);

/// How long a run waits for someone else's publication before giving up.
///
/// Deliberately longer than [`STALE`]: a waiter that reaches this has not lost
/// a race to a run that was working — it has been sitting behind a lock nobody
/// is refreshing and nobody has released, which is a wedged filesystem or a
/// directory somebody else's program is writing. That deserves a message, not
/// more patience.
const WAIT: Duration = Duration::from_secs(600);

/// How often the wait looks again.
///
/// Short enough that the second of two runs starts within a tenth of a second
/// of the first finishing; long enough that waiting costs nothing measurable.
const POLL: Duration = Duration::from_millis(100);

/// The lock file's name, beside the package rather than inside it.
///
/// Inside would be wrong twice over: the directory is renamed out from under
/// itself during publication, and the package is meant to be a corpus and
/// nothing else — a reader who opens it should find documents and an inventory,
/// not this program's bookkeeping.
fn lock_path(destination: &Path) -> PathBuf {
    destination.join(format!(".{PACKAGE}.publish.lock"))
}

/// Held for the whole of a publication; released when it is dropped.
#[derive(Debug)]
pub(super) struct Publication {
    path: PathBuf,
    /// What this run wrote into the file, so the guard can tell its own lock
    /// from one that replaced it.
    token: String,
}

impl Publication {
    /// Take the lock in `destination`, waiting for whoever holds it.
    pub(super) fn acquire(destination: &Path, job: &Job<'_>) -> Result<Self> {
        acquire_within(destination, job, STALE, WAIT, POLL)
    }
}

/// The three durations as arguments, so the waiting can be tested in
/// milliseconds rather than minutes.
fn acquire_within(
    destination: &Path,
    job: &Job<'_>,
    stale: Duration,
    wait: Duration,
    poll: Duration,
) -> Result<Publication> {
    let path = lock_path(destination);
    let token = token();
    let deadline = SystemTime::now() + wait;

    loop {
        // Cancellation is checked before each attempt, so a run stopped while
        // waiting comes out as a stop at the phase it was waiting in — not as a
        // lock failure, which is what it would look like from the outside.
        job.check(Phase::Publishing)?;

        // **The lock file is not left empty by a kill.**
        //
        // It used to be created with `create_new` and written a line later, and
        // a run killed between those two calls left a nameless zero-byte lock:
        // `holder` had nothing to report, and every other run waited out the
        // full five-minute staleness before it could publish. The token is now
        // written into a scratch file beside the destination and moved into
        // place with `rename`, which is atomic on one filesystem — the same
        // move the export uses for everything else it publishes.
        match write_token(&path, &token) {
            Ok(()) => {
                // Read back what is on disk. Two runs that both judged an
                // abandoned lock stale in the same poll can each remove a file
                // and each create one — and one of the removals may be of the
                // other's fresh lock. The window is small and the situation is
                // rare, but the check that closes it is one read of a file this
                // run has just written: if it does not say what this run said,
                // this run does not hold the lock.
                match fs::read_to_string(&path) {
                    Ok(found) if found == token => {
                        return Ok(Publication { path, token });
                    }
                    _ => continue,
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                // Read the holder *before* judging its age, so that what is
                // removed is the file that was judged. Between the judgement
                // and the removal the holder can finish and a third run can
                // take the lock: removing then would hand the directory to two
                // runs at once. Comparing what is on disk with what was read a
                // moment ago does not close that window — nothing short of an
                // atomic compare-and-delete would — but it turns it from the
                // whole staleness check into two adjacent syscalls, and a lock
                // that was replaced in between survives.
                let seen = fs::read_to_string(&path).ok();
                if abandoned(&path, stale) {
                    // Not `?` in either arm: a removal that fails because
                    // someone else got there first is the normal outcome of two
                    // waiters, and the next attempt is what settles it.
                    match seen {
                        Some(seen) => remove_if_unchanged(&path, &seen),
                        // Unreadable, which is the case `abandoned` already
                        // treats as abandoned: either not ours or broken, and
                        // there is nothing to compare it against.
                        None => {
                            let _ = fs::remove_file(&path);
                        }
                    }
                    continue;
                }
                if SystemTime::now() >= deadline {
                    return Err(ArunaError::PublishBusy {
                        path: path.clone(),
                        holder: holder(&path),
                    });
                }
                std::thread::sleep(poll);
            }
            Err(source) => {
                return Err(ArunaError::Io {
                    path: path.clone(),
                    source,
                })
            }
        }
    }
}

/// Whether the lock on disk is old enough to be treated as left behind.
///
/// Put `token` at `path`, atomically, failing if something is already there.
///
/// `create_new` on the scratch file keeps two runs from sharing it, and the
/// rename is what makes the lock appear whole or not at all. `rename` replaces
/// an existing file silently, so the `create_new` that decides who holds the
/// lock has to be the one on the destination — hence the check below it: a
/// scratch file that renames over somebody else's lock would hand the directory
/// to two runs.
fn write_token(path: &Path, token: &str) -> std::io::Result<()> {
    let scratch = crate::paths::scratch_sibling(path);
    {
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&scratch)?;
        file.write_all(token.as_bytes())?;
    }
    // The gate: whoever creates this empty marker owns the name. Only then is
    // the token moved onto it.
    //
    // One thing this shape does not promise, said here rather than left to be
    // discovered: between this line and the rename the file exists and is
    // empty, so a reader in that window learns nothing from `holder` — it waits
    // and retries, which is correct, but the message it would print is blank.
    // The window is two syscalls wide and costs a blank line in a diagnostic,
    // not a wait.
    //
    // What it does promise is that nothing is left behind. A rename that fails
    // used to leave the empty marker in place, and every other run then waited
    // out the full staleness on a lock nobody held — the very failure this
    // function exists to prevent, moved to a rarer path. Both files go now.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(_) => {}
        Err(err) => {
            let _ = fs::remove_file(&scratch);
            return Err(err);
        }
    }
    let renamed = fs::rename(&scratch, path);
    if renamed.is_err() {
        // Both, and in this order: the marker is what would block other runs,
        // the scratch file is only litter.
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(&scratch);
    }
    renamed
}

/// Remove `path` only if it still holds what was read from it.
///
/// The narrow half of the race above: a lock that was replaced between the
/// staleness judgement and this call belongs to whoever holds it now, and
/// removing it would let two runs publish at once. The read here is not atomic
/// with the removal, so this narrows the window rather than closing it — which
/// is the honest description and the reason it is a named function with a test
/// rather than three lines inside the loop.
fn remove_if_unchanged(path: &Path, seen: &str) {
    if matches!(fs::read_to_string(path), Ok(found) if found == seen) {
        let _ = fs::remove_file(path);
    }
}

/// A file whose age cannot be read at all is treated as abandoned: it is either
/// gone — in which case the next attempt takes it — or on a filesystem that
/// cannot answer, where waiting for an answer that never comes is worse than
/// proceeding.
fn abandoned(path: &Path, stale: Duration) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return true;
    };
    let Ok(modified) = meta.modified() else {
        return true;
    };
    modified.elapsed().map(|age| age >= stale).unwrap_or(false)
}

/// What the lock file says about who holds it, for the error message.
fn holder(path: &Path) -> String {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "an unnamed run".to_string())
}

/// This run, in a form a person reading the file can act on.
///
/// The process id is not used for a liveness check — nothing here can make one
/// — but it is what turns "something holds the lock" into "process 4711 does",
/// which is a question `ps` can answer and a person can decide about. The
/// second half makes the string unique per attempt, which is what the read-back
/// above compares.
fn token() -> String {
    let pid = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "pid {pid}, since {}.{:09}\n",
        now.as_secs(),
        now.subsec_nanos()
    )
}

impl Drop for Publication {
    fn drop(&mut self) {
        // Only if it is still this run's lock. A stale lock this run's own file
        // replaced has the same name, and removing one that belongs to whoever
        // holds it now would hand the directory to a third run mid-publication.
        if matches!(fs::read_to_string(&self.path), Ok(found) if found == self.token) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn instant() -> (Duration, Duration, Duration) {
        // A stale threshold no lock reaches, a wait short enough to fail a test
        // quickly, and a poll shorter still.
        (
            Duration::from_secs(3600),
            Duration::from_millis(60),
            Duration::from_millis(5),
        )
    }

    /// One run publishes at a time, and the one that cannot say so names who
    /// can. `holder` is the whole point of the message: "busy" is not a
    /// diagnosis, "pid 4711 since …" is.
    /// **Убийство между созданием и записью не оставляет пустого файла, а
    /// попытка занять чужой не оставляет следов.**
    ///
    /// Отказ, ради которого сделана правка, наблюдается только под убийством
    /// процесса между созданием файла и записью токена, и тестом это не
    /// разыгрывается. Разыгрывается то, что правка добавила и что можно
    /// проверить: имя занято – отказ, и рядом не остается ни одного рабочего
    /// файла, а занятое имя содержит токен целиком, а не пустоту.
    #[test]
    fn a_lock_is_written_whole_or_not_at_all() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".aruna-publish.lock");

        write_token(&path, "pid 1, since 1.0\n").expect("free name is taken");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "pid 1, since 1.0\n",
            "имя занято, но токена в файле нет"
        );

        let taken = write_token(&path, "pid 2, since 2.0\n");
        assert!(taken.is_err(), "занятое имя было перезаписано");
        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            "pid 1, since 1.0\n",
            "чужой токен был затерт"
        );

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n != ".aruna-publish.lock")
            .collect();
        assert!(
            leftovers.is_empty(),
            "неудачная попытка оставила после себя {leftovers:?}"
        );
    }

    /// **A lock replaced under our feet is not removed.**
    ///
    /// The whole point of [`remove_if_unchanged`]: the run that judged a lock
    /// stale must not delete the fresh one that took its place while it was
    /// deciding.
    #[test]
    fn a_lock_that_changed_hands_is_left_alone() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(".aruna-publish.lock");

        fs::write(&path, "pid 1, since 1.0\n").expect("write");
        remove_if_unchanged(&path, "pid 1, since 1.0\n");
        assert!(!path.exists(), "the lock we read is the lock we remove");

        fs::write(&path, "pid 2, since 2.0\n").expect("write");
        remove_if_unchanged(&path, "pid 1, since 1.0\n");
        assert!(
            path.exists(),
            "a lock taken by another run between the judgement and the removal was deleted"
        );
    }

    #[test]
    fn a_second_publication_waits_and_then_says_who_holds_it() {
        let dir = tempdir().expect("tempdir");
        let (stale, wait, poll) = instant();

        let held = acquire_within(dir.path(), &Job::unattended(), stale, wait, poll)
            .expect("the first run takes it");

        match acquire_within(dir.path(), &Job::unattended(), stale, wait, poll) {
            Err(ArunaError::PublishBusy { path, holder }) => {
                assert_eq!(path, lock_path(dir.path()));
                assert!(holder.contains("pid"), "no diagnosis in {holder:?}");
            }
            other => panic!("expected a busy lock, got {other:?}"),
        }

        drop(held);
        acquire_within(dir.path(), &Job::unattended(), stale, wait, poll)
            .expect("released when the first run is done");
    }

    /// A run killed mid-publication leaves its lock behind, and the next run
    /// must not wait for a process that will never release it.
    #[test]
    fn a_lock_left_behind_is_taken_once_it_goes_stale() {
        let dir = tempdir().expect("tempdir");
        let path = lock_path(dir.path());
        fs::write(&path, "pid 1, since long ago\n").expect("an abandoned lock");

        let (_, wait, poll) = instant();
        let taken = acquire_within(
            dir.path(),
            &Job::unattended(),
            // Everything already written is old enough.
            Duration::ZERO,
            wait,
            poll,
        )
        .expect("an abandoned lock is taken");

        assert_eq!(
            fs::read_to_string(&path).expect("read"),
            taken.token,
            "the lock now says who holds it"
        );
    }

    /// The guard removes its own lock and nobody else's.
    ///
    /// The distinction matters exactly once: when this run's lock has already
    /// been taken over by another, and dropping the guard would otherwise hand
    /// the directory to a third run in the middle of a publication.
    #[test]
    fn dropping_a_guard_whose_lock_was_replaced_leaves_the_new_one_alone() {
        let dir = tempdir().expect("tempdir");
        let (stale, wait, poll) = instant();
        let path = lock_path(dir.path());

        let held =
            acquire_within(dir.path(), &Job::unattended(), stale, wait, poll).expect("acquire");
        fs::write(&path, "pid 999, someone else\n").expect("replaced");
        drop(held);

        assert!(
            path.exists(),
            "a guard removed a lock that was no longer its own"
        );
    }

    /// Waiting is not a place a cancelled run gets stuck in, and the stop it
    /// reports is a stop rather than a lock failure.
    #[test]
    fn a_run_cancelled_while_waiting_reports_the_stop() {
        let dir = tempdir().expect("tempdir");
        let (stale, wait, poll) = instant();
        let _held = acquire_within(dir.path(), &Job::unattended(), stale, wait, poll)
            .expect("the lock is held by someone");

        let cancel = crate::job::Cancel::new();
        cancel.cancel();
        let job = Job::new(&crate::progress::Silent, &cancel);

        match acquire_within(dir.path(), &job, stale, Duration::from_secs(30), poll) {
            Err(ArunaError::Cancelled { phase }) => assert_eq!(phase, Phase::Publishing),
            other => panic!("expected a stop, got {other:?}"),
        }
    }
}
