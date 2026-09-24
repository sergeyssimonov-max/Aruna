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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::PACKAGE;

/// How long a run waits for someone else's publication before giving up.
///
/// A publication moves a directory, renames another onto its name and reads
/// back 23 936 files – two to six seconds on the real corpus. A holder that is
/// still alive after ten minutes is not publishing: it is a wedged filesystem
/// or a run stopped in a debugger, and that deserves a message, not more
/// patience. A holder that is dead is not waited for at all – see
/// [`Publication`].
const WAIT: Duration = Duration::from_secs(600);

/// How often the wait looks again.
///
/// Short enough that the second of two runs starts within a tenth of a second
/// of the first finishing; long enough that waiting costs nothing measurable.
const POLL: Duration = Duration::from_millis(100);

/// Дальше этого предела файл блокировки не читается.
///
/// Токен – одна короткая строка. Файл длиннее предела не может быть токеном
/// этого прогона ни при каком содержимом, поэтому дочитывать его незачем: имя
/// `.{PACKAGE}.publish.lock` лежит в каталоге назначения, положить туда файл
/// произвольного размера может любой процесс, а прочитанное идет в текст
/// отказа. Предел с большим запасом: настоящий токен – около сорока байт.
const MAX_LOCK: u64 = 4096;

/// Файл блокировки, прочитанный не дальше [`MAX_LOCK`].
///
/// `None` значит «это не токен»: файла нет, он длиннее предела, он не текст
/// или это не обычный файл. **Открывается только обычный файл**: именованный
/// канал под этим именем держал бы `open` до появления писателя. Между
/// проверкой и открытием остается окно в два системных вызова; закрыть его
/// целиком мог бы только неблокирующий `open`, а его флага в стандартной
/// библиотеке нет.
fn read_token(path: &Path) -> Option<String> {
    use std::io::Read as _;
    if !fs::symlink_metadata(path).ok()?.file_type().is_file() {
        return None;
    }
    let file = fs::File::open(path).ok()?;
    let mut text = String::new();
    // На один байт больше предела: длину читаем, чтобы отличить «ровно предел»
    // от «предел и еще сколько-то», а не чтобы вернуть лишнее.
    file.take(MAX_LOCK + 1).read_to_string(&mut text).ok()?;
    (text.len() as u64 <= MAX_LOCK).then_some(text)
}

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
///
/// **The lock is the kernel's, not the file's.** The guard holds the file open
/// under an exclusive `flock`, and the kernel releases that when the process
/// ends, however it ends. Until 2026-09-24 the lock was the file's existence,
/// judged abandoned by its age: a run killed while publishing – or between
/// creating the file and writing it – left a file every later run waited five
/// silent minutes for (measured 300,4 and 301,8 s on 24.09), and a directory
/// under the name was waited for ten. Now a file nobody holds is free at once,
/// whatever it says, and anything under the name that is not a regular file is
/// refused at once. The same mechanism has held the staging marker since
/// 13.09.2026 (`Owner` in the parent module).
///
/// The token written into the file is diagnosis only: it is what turns "busy"
/// into "process 4711", a question `ps` can answer.
#[derive(Debug)]
pub(super) struct Publication {
    path: PathBuf,
    /// Held open for as long as the guard lives: closing it releases the lock.
    file: fs::File,
}

impl Publication {
    /// Take the lock in `destination`, waiting for whoever holds it.
    pub(super) fn acquire(destination: &Path, job: &Job<'_>) -> Result<Self> {
        acquire_within(destination, job, WAIT, POLL)
    }
}

/// The two durations as arguments, so the waiting can be tested in
/// milliseconds rather than minutes.
fn acquire_within(
    destination: &Path,
    job: &Job<'_>,
    wait: Duration,
    poll: Duration,
) -> Result<Publication> {
    let path = lock_path(destination);
    let started = Instant::now();
    let mut told = false;

    loop {
        // Cancellation is checked before each attempt, so a run stopped while
        // waiting comes out as a stop at the phase it was waiting in — not as a
        // lock failure, which is what it would look like from the outside.
        job.check(Phase::Publishing)?;

        // Not a regular file under the name – a directory, a pipe, a link –
        // is nothing this program put there and nothing that will ever be
        // released: refused now rather than after the whole wait.
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.file_type().is_file() {
                return Err(ArunaError::ExportDestination {
                    path,
                    reason:
                        "something that is not a lock file stands where the publish lock belongs"
                            .to_string(),
                });
            }
        }

        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(ArunaError::io(&path))?;

        match file.try_lock() {
            Ok(()) => {
                // **The name must still be the file this run locked.** A holder
                // that finished removes the file while holding the lock, and a
                // waiter that had opened it before then locks a file with no
                // name: it holds nothing anyone else can see. The same inode
                // under the name is the proof; anything else is another try.
                if !same_file(&file, &path) {
                    continue;
                }
                write_token(&file).map_err(ArunaError::io(&path))?;
                return Ok(Publication { path, file });
            }
            Err(fs::TryLockError::WouldBlock) => {
                if !told {
                    job.report(crate::progress::Event::WaitingForPublication);
                    told = true;
                }
                if started.elapsed() >= wait {
                    return Err(ArunaError::PublishBusy {
                        path: path.clone(),
                        holder: holder(&path),
                    });
                }
                drop(file);
                std::thread::sleep(poll);
            }
            Err(fs::TryLockError::Error(source)) => {
                return Err(ArunaError::Io { path, source });
            }
        }
    }
}

/// Whether `path` names the file `file` has open.
#[cfg(unix)]
fn same_file(file: &fs::File, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    match (file.metadata(), fs::symlink_metadata(path)) {
        (Ok(held), Ok(named)) => held.dev() == named.dev() && held.ino() == named.ino(),
        _ => false,
    }
}

/// This run's token into the file it has just locked, replacing whatever was
/// there – a token of a run that is gone, or nothing.
fn write_token(file: &fs::File) -> std::io::Result<()> {
    use std::io::{Seek as _, Write as _};
    let mut file = file;
    file.set_len(0)?;
    file.seek(std::io::SeekFrom::Start(0))?;
    file.write_all(token().as_bytes())
}

/// What the lock file says about who holds it, for the error message.
///
/// **Only a token this program writes is repeated, and nothing else.** The
/// name lies in the reader's folder and anything can be put there; until
/// 2026-09-13 whatever it held went into the refusal after a `trim`, and the
/// release gate saw ESC sequences, BEL and NUL reach the terminal from a file
/// it had planted. Filtering characters would still repeat a stranger's words
/// in this program's voice, so the shape of [`token`] is the whole test.
fn holder(path: &Path) -> String {
    match read_token(path) {
        Some(text) if is_token(text.trim()) => text.trim().to_string(),
        Some(_) => "a lock file this program did not write".to_string(),
        None => "an unnamed run".to_string(),
    }
}

/// Whether `text` has the shape [`token`] gives it: `pid <digits>, since
/// <digits>.<digits>`.
fn is_token(text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let Some(rest) = text.strip_prefix("pid ") else {
        return false;
    };
    let Some((pid, since)) = rest.split_once(", since ") else {
        return false;
    };
    let Some((seconds, nanos)) = since.split_once('.') else {
        return false;
    };
    digits(pid) && digits(seconds) && digits(nanos)
}

/// This run, in a form a person reading the file can act on.
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
        // Removed while still held, and only if the name is still this file:
        // a waiter that locks the removed file afterwards sees that it has no
        // name and tries again. Nothing is removed that another run holds.
        if same_file(&self.file, &self.path) {
            let _ = fs::remove_file(&self.path);
        }
        // The lock goes with the descriptor, when `file` is dropped after this.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// A wait short enough to fail a test quickly, and a poll shorter still.
    fn instant() -> (Duration, Duration) {
        (Duration::from_millis(60), Duration::from_millis(5))
    }

    /// Someone else's hold on the lock file: the file open under an exclusive
    /// `flock`, with `content` in it, for as long as the returned file lives.
    fn held_by_another(destination: &Path, content: &str) -> fs::File {
        let path = lock_path(destination);
        fs::write(&path, content).expect("write");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open");
        file.try_lock().expect("lock");
        file
    }

    /// A sink that remembers how often it heard that the run is waiting.
    #[derive(Default)]
    struct Heard(std::sync::atomic::AtomicUsize);

    impl crate::progress::Progress for Heard {
        fn report(&self, event: crate::progress::Event<'_>) {
            if matches!(event, crate::progress::Event::WaitingForPublication) {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    /// One run publishes at a time, and the one that cannot say so names who
    /// can. `holder` is the whole point of the message: "busy" is not a
    /// diagnosis, "pid 4711 since …" is.
    #[test]
    fn a_second_publication_waits_and_then_says_who_holds_it() {
        let dir = tempdir().expect("tempdir");
        let (wait, poll) = instant();

        let held = acquire_within(dir.path(), &Job::unattended(), wait, poll)
            .expect("the first run takes it");

        match acquire_within(dir.path(), &Job::unattended(), wait, poll) {
            Err(ArunaError::PublishBusy { path, holder }) => {
                assert_eq!(path, lock_path(dir.path()));
                assert!(is_token(&holder), "no diagnosis in {holder:?}");
            }
            other => panic!("expected a busy lock, got {other:?}"),
        }

        drop(held);
        assert!(
            !lock_path(dir.path()).exists(),
            "the lock file outlived its publication"
        );
        acquire_within(dir.path(), &Job::unattended(), wait, poll)
            .expect("released when the first run is done");
    }

    /// **A run that waits says so, once.**
    ///
    /// Until 2026-09-24 a waiting run said nothing for as long as it waited –
    /// five minutes behind a lock left by a killed run. The wait is short now,
    /// but a live holder can still be waited for, and the reader hears it.
    #[test]
    fn a_run_that_waits_says_so_once() {
        let dir = tempdir().expect("tempdir");
        let _other = held_by_another(dir.path(), "pid 1, since 1.0\n");
        let heard = Heard::default();
        let cancel = crate::job::Cancel::new();
        let job = Job::new(&heard, &cancel);

        let (wait, poll) = instant();
        let outcome = acquire_within(dir.path(), &job, wait, poll);
        assert!(matches!(outcome, Err(ArunaError::PublishBusy { .. })));
        assert_eq!(
            heard.0.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the wait was announced other than once"
        );
    }

    /// **A lock whose run is gone is taken at once, whatever the file says.**
    ///
    /// C2 and C3 of the reliability runs: a run killed while publishing left its
    /// token, and one killed between creating the file and writing it left an
    /// empty file; the next run waited out a five-minute staleness either way
    /// (300,4 and 301,8 s, measured 24.09.2026). Nobody holds the kernel's lock
    /// on either file, so either is free now. On the old code both waited – here
    /// the wait is 60 ms, and the old code refused with `PublishBusy`.
    #[test]
    fn a_lock_whose_run_is_gone_is_taken_at_once() {
        for left in ["pid 1, since 1.0\n", "", "(not a token at all)"] {
            let dir = tempdir().expect("tempdir");
            let path = lock_path(dir.path());
            fs::write(&path, left).expect("left behind");

            let (wait, poll) = instant();
            let taken = acquire_within(dir.path(), &Job::unattended(), wait, poll)
                .unwrap_or_else(|e| panic!("a lock nobody holds ({left:?}) was waited for: {e}"));

            let now = fs::read_to_string(&path).expect("read");
            assert!(
                is_token(now.trim()),
                "the lock does not say who holds it: {now:?}"
            );
            drop(taken);
        }
    }

    /// **Файл под именем блокировки читается не дальше предела.**
    ///
    /// Имя лежит в пользовательском каталоге назначения, и держатель, чей файл
    /// длиннее любого токена, – не эта программа. В отказ из такого файла
    /// уходит не больше предела и не его содержимое.
    #[test]
    fn a_lock_file_of_any_size_is_read_no_further_than_the_limit() {
        let dir = tempdir().expect("tempdir");
        let huge = "x".repeat(2 * 1024 * 1024);
        let _other = held_by_another(dir.path(), &huge);

        let (wait, poll) = instant();
        match acquire_within(dir.path(), &Job::unattended(), wait, poll) {
            Err(ArunaError::PublishBusy { holder, .. }) => {
                assert!(
                    holder.len() <= MAX_LOCK as usize,
                    "в отказ уехало {} байт файла блокировки",
                    holder.len()
                );
            }
            other => panic!("ожидался занятый замок, получено {other:?}"),
        }
        assert_eq!(
            fs::metadata(lock_path(dir.path())).expect("metadata").len(),
            huge.len() as u64,
            "файл держателя изменен"
        );
    }

    /// The guard removes its own lock file and nobody else's.
    ///
    /// When the name no longer points at the file this run locked – replaced
    /// by another file – the guard leaves the new one alone.
    #[test]
    fn dropping_a_guard_whose_file_was_replaced_leaves_the_new_one_alone() {
        let dir = tempdir().expect("tempdir");
        let (wait, poll) = instant();
        let path = lock_path(dir.path());

        let held = acquire_within(dir.path(), &Job::unattended(), wait, poll).expect("acquire");
        let other = dir.path().join("other");
        fs::write(&other, "pid 999, since 9.0\n").expect("write");
        fs::rename(&other, &path).expect("replaced");
        drop(held);

        assert!(
            path.exists(),
            "a guard removed a lock file that was no longer its own"
        );
    }

    /// **A waiter that locked a file its holder removed tries again.**
    ///
    /// The holder removes the file while it still holds the lock; a waiter
    /// that had it open locks a file with no name. The inode check sends it
    /// back to the name, where it takes the lock properly.
    #[test]
    fn a_lock_on_a_removed_file_is_not_taken_for_the_lock() {
        let dir = tempdir().expect("tempdir");
        let path = lock_path(dir.path());
        fs::write(&path, "").expect("write");
        let stale = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open");
        fs::remove_file(&path).expect("removed by its holder");
        stale.try_lock().expect("the nameless file locks");
        assert!(
            !same_file(&stale, &path),
            "a nameless file passed for the lock"
        );

        let (wait, poll) = instant();
        let taken =
            acquire_within(dir.path(), &Job::unattended(), wait, poll).expect("the name is free");
        assert!(same_file(&taken.file, &path));
    }

    /// Waiting is not a place a cancelled run gets stuck in, and the stop it
    /// reports is a stop rather than a lock failure.
    #[test]
    fn a_run_cancelled_while_waiting_reports_the_stop() {
        let dir = tempdir().expect("tempdir");
        let (_, poll) = instant();
        let _other = held_by_another(dir.path(), "pid 1, since 1.0\n");

        let cancel = crate::job::Cancel::new();
        cancel.cancel();
        let job = Job::new(&crate::progress::Silent, &cancel);

        match acquire_within(dir.path(), &job, Duration::from_secs(30), poll) {
            Err(ArunaError::Cancelled { phase }) => assert_eq!(phase, Phase::Publishing),
            other => panic!("expected a stop, got {other:?}"),
        }
    }

    /// **Отказ не повторяет содержимое чужого файла.**
    ///
    /// Заслон 13.09.2026, позиция 3: файл с ESC, BEL, BS и NUL внутри уехал в
    /// текст отказа целиком, и терминал читателя исполнил цветовые
    /// последовательности. В отказ идет только собственный токен программы.
    #[test]
    fn a_foreign_lock_file_is_not_repeated_into_the_refusal() {
        let dir = tempdir().expect("tempdir");
        let foreign = "(FOREIGN-CONTENT-3f9a\u{1b}[31mRED\u{1b}[0m\u{7}\u{8}\u{0}tail)";
        let _other = held_by_another(dir.path(), foreign);

        let (wait, poll) = instant();
        match acquire_within(dir.path(), &Job::unattended(), wait, poll) {
            Err(ArunaError::PublishBusy { holder, .. }) => {
                assert!(
                    !holder.chars().any(char::is_control),
                    "управляющие знаки в отказе: {holder:?}"
                );
                assert!(
                    !holder.contains("FOREIGN-CONTENT"),
                    "чужое содержимое в отказе: {holder:?}"
                );
            }
            other => panic!("ожидался занятый замок, получено {other:?}"),
        }
        assert_eq!(
            fs::read_to_string(lock_path(dir.path())).expect("read"),
            foreign,
            "файл держателя тронут"
        );
    }

    /// **Не файл под именем блокировки – отказ сразу, а не ожидание.**
    ///
    /// Каталог ждался десять минут (610 с на заслоне 23.09.2026), канал держал
    /// `open` без конца до 23.09.2026, ссылка вела бы запись туда, куда она
    /// указывает. Ни одно из них этой программой не положено и никогда не
    /// освободится; отказ называет место и ничего не трогает.
    ///
    /// Вызов идет в своем потоке с предохранителем: на коде с дефектом канал не
    /// возвращается никогда, а каталог – только через весь срок.
    #[test]
    fn something_that_is_not_a_lock_file_is_refused_at_once() {
        for kind in ["directory", "fifo", "symlink"] {
            let dir = tempdir().expect("tempdir");
            let path = lock_path(dir.path());
            match kind {
                "directory" => fs::create_dir(&path).expect("dir"),
                "fifo" => {
                    let made = std::process::Command::new("mkfifo")
                        .arg(&path)
                        .status()
                        .expect("mkfifo");
                    assert!(made.success(), "канал не создан");
                }
                _ => std::os::unix::fs::symlink(dir.path().join("elsewhere"), &path)
                    .expect("symlink"),
            }

            let destination = dir.path().to_path_buf();
            let (sent, received) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let outcome = acquire_within(
                    &destination,
                    &Job::unattended(),
                    Duration::from_secs(3600),
                    Duration::from_millis(5),
                )
                .map(|_| ());
                let _ = sent.send(outcome);
            });

            match received.recv_timeout(Duration::from_secs(5)) {
                Ok(Err(ArunaError::ExportDestination { path: named, .. })) => {
                    assert_eq!(named, path, "{kind}: the refusal names another place");
                }
                Err(_) => panic!("{kind}: no answer within 5 s"),
                Ok(other) => panic!("{kind}: expected a refusal, got {other:?}"),
            }
            assert!(
                fs::symlink_metadata(&path).is_ok(),
                "{kind} under the lock's name was removed"
            );
            assert!(
                !dir.path().join("elsewhere").exists(),
                "{kind}: something was written where the link points"
            );
        }
    }
}
