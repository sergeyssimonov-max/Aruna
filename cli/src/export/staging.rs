//! A build until it is published: the staging directory beside the
//! destination, the marker that says whose it is, the package that was already
//! there held aside, and the sweep that clears what an earlier run abandoned.
//!
//! Moved out of `export/mod.rs` on 2026-10-01 without a change of logic: these
//! guards are what make a failed or cancelled build leave the destination as
//! it was found, and they are read together.

use super::{create_dir, remove_dir, PACKAGE};
use crate::error::{ArunaError, Result};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// The package that was already there, held aside until its replacement is in
/// place.
///
/// Deleting it first meant that between the delete and the rename there was no
/// package at all, and for a 389 MB tree that gap is seconds long rather than
/// instants — a run interrupted inside it left the reader with neither the old
/// package nor the new one. A rename is atomic, so the gap is now one syscall
/// wide.
///
/// A guard rather than a sequence of statements in [`build`], for the reason
/// [`Staging`] and [`crate::paths::write_atomic`] are: cleanup that runs only
/// on the paths the author remembered is cleanup that stops running the moment
/// someone adds a `?`. Dropping without [`committed`](Self::committed) puts the
/// old package back.
pub(super) struct Replaced {
    target: PathBuf,
    /// The package this run moved out of the way. Put back by [`Drop`].
    aside: Option<PathBuf>,
    /// An aside copy an earlier run left behind and this one did not make.
    ///
    /// It exists when a process was killed between the rename that moved a
    /// package aside and the one that published its replacement — the window
    /// `Drop` cannot cover, because a kill runs no destructor. Nothing had ever
    /// cleared it: [`aside`](Self::aside) only looks at that path when there is
    /// a package at `target` to move, and after a kill there is not. The copy
    /// then sat in the destination through the next build — a second, hidden
    /// package the size of the real one — and was only swept up by the build
    /// after that.
    ///
    /// Kept apart from `aside` rather than merged into it because the two are
    /// owed different things. This one is the reader's only remaining copy of
    /// the package they had, so it is removed once a new one is safely
    /// published and never before — and [`Drop`] must not rename it onto
    /// `target`, which would be restoring something this run did not move.
    stale: Option<PathBuf>,
    /// This run's tree stands under `target`: a refusal from here on takes it
    /// down again, whether or not there was a reader's copy to put back.
    published: bool,
    committed: bool,
}

impl Replaced {
    /// Move whatever is at `target` out of the way, if anything is.
    ///
    /// `symlink_metadata`, not `exists`: `exists` follows links, and a link is
    /// what would be renamed here.
    pub(super) fn aside(target: &Path, destination: &Path) -> Result<Self> {
        let mut held = Self {
            target: target.to_path_buf(),
            aside: None,
            stale: None,
            published: false,
            committed: false,
        };
        let aside = destination.join(format!(".{PACKAGE}.previous"));
        if fs::symlink_metadata(target).is_err() {
            // Nothing to move. Anything under the aside name is an earlier
            // run's, orphaned by a kill; see [`Replaced::stale`].
            if aside.exists() {
                held.stale = Some(aside);
            }
            return Ok(held);
        }
        if aside.exists() {
            remove_dir(&aside)?;
        }
        fs::rename(target, &aside).map_err(ArunaError::io(&target))?;
        held.aside = Some(aside);
        Ok(held)
    }

    /// This run's tree has just taken `target`, and is not yet confirmed.
    pub(super) fn published(&mut self) {
        self.published = true;
    }

    /// The replacement is in place; the old copy is now only occupying space.
    ///
    /// Failing to remove it is not a reason to fail a build that worked, so
    /// whatever is left is handed back for [`build`] to report. Returned rather
    /// than printed here: this guard exists to be correct on every exit path,
    /// and a sink threaded through it for one line would have to reach [`Drop`]
    /// too, where there is no caller left to tell.
    ///
    /// Both copies go: the one this run moved aside, and an orphan an earlier
    /// killed run left. Now is the moment for both — a new package is in place,
    /// so neither is anybody's last copy of anything any more.
    pub(super) fn committed(mut self) -> Vec<PathBuf> {
        self.committed = true;
        self.aside
            .iter()
            .chain(self.stale.iter())
            .filter(|path| remove_dir(path).is_err())
            .cloned()
            .collect()
    }
}

impl Drop for Replaced {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(aside) = &self.aside {
            // **Имя может быть занято, и снять занявшее его дерево здесь
            // правильно.** Сюда приходят и отказы после публикации: под именем
            // пакета в этот момент стоит то, что положил этот прогон, а
            // читательская копия лежит в `aside`. Один `rename` на занятое имя
            // отвечает `ENOTEMPTY` и молча ничего не делает — читатель
            // остается с непроверенным пакетом под своим именем и со своим
            // собственным под точечным, которого он не видит.
            //
            // Убирается только то, что этот прогон сам туда поставил: `aside`
            // не `None` лишь после удавшегося переименования с этого имени,
            // после которого занять его мог только `Staging::publish` этого
            // прогона. Чужой прогон Aruna сюда не встанет — блокировка
            // публикации объявлена раньше `Replaced` и потому снимается позже.
            if fs::symlink_metadata(&self.target).is_ok() {
                let _ = fs::remove_dir_all(&self.target);
            }
            // Best effort, and the only thing left worth doing: the run has
            // already failed, and putting the reader's package back matters
            // more than reporting why the restore failed too.
            let _ = fs::rename(aside, &self.target);
        } else if self.published {
            // **Первая сборка, и она отказала после публикации.** Возвращать
            // нечего, но и оставлять нечего: под именем пакета стоит дерево,
            // которое проверка только что отвергла, и читатель принял бы его за
            // пакет. До 25.09.2026 оно так и оставалось – `Drop` действовал
            // лишь при `aside`. Копию, брошенную убитым прогоном (`stale`), это
            // не трогает: она остается ждать следующей удачной сборки.
            let _ = fs::remove_dir_all(&self.target);
        }
    }
}

/// The half-built package: a directory that removes itself unless it is
/// published.
///
/// Modelled on [`crate::download::Scratch`], and for the same reason. A build
/// that fails after writing part of the corpus used to leave the staging
/// directory behind — up to 372 MB of a package nobody asked for, cleared only
/// if the next build happened to use the same destination. Every `?` between
/// creation and the rename is now covered by going out of scope.
///
/// What going out of scope cannot cover is a kill, and for that the directory
/// carries an [`Owner`]: claimed before the directory exists, held until it is
/// published or removed, and what [`sweep_abandoned_staging`] asks before it
/// deletes anything. Fields drop in order, so the marker outlives the
/// directory it vouches for by the length of `Drop`.
pub(super) struct Staging {
    path: PathBuf,
    published: bool,
    _owner: Option<Owner>,
}

impl Staging {
    /// An empty staging directory, clearing whatever an earlier run left.
    pub(super) fn fresh(path: PathBuf) -> Result<Self> {
        let owner = Owner::claim(owner_marker(&path));
        if path.exists() {
            remove_dir(&path)?;
        }
        create_dir(&path)?;
        Ok(Self {
            path,
            published: false,
            _owner: owner,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Give the finished package its name. After this there is nothing to clean.
    pub(super) fn publish(mut self, destination: &Path) -> Result<()> {
        fs::rename(&self.path, destination).map_err(ArunaError::io(&destination))?;
        self.published = true;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.published {
            // A failure to tidy up is not worth failing a run that has already
            // failed, and there is nobody left to tell.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// The name this run stages under: unique to the run, not to the destination.
///
/// **It used to be `.{PACKAGE}.build`, one name for every run, and that was
/// safe only while nothing ran two builds into one destination.** Since 2.2.0
/// the binary itself exports on every run, so two of them — a second
/// double-click — meet in the reader's Downloads folder. Measured with the
/// fixed name: each run's [`Staging::fresh`] cleared the other's directory and
/// each `Drop` removed what the other was writing into, and **both runs failed
/// leaving no package at all**, where before the export was wired in both had
/// succeeded.
///
/// The same shape as [`crate::paths::scratch_sibling`], and for the same
/// reason: process id, plus a counter so one process can stage twice. It is now
/// literally the same — both take it from [`crate::paths::run_tag`], which is
/// where that shape is decided.
///
/// **What that took away, and how it came back.** A run killed with a signal
/// leaves its staging directory behind, and a unique name means the next run
/// no longer finds it under the name it would have used itself. From 2.2.0
/// until 13.09.2026 the next run simply built beside it, and the release gate
/// measured the cost: 130.5 MiB left by a kill while writing, a whole second
/// package, 382.8 MiB, by a kill at the start of publishing — each surviving
/// every later successful build. [`sweep_abandoned_staging`] now removes them,
/// and tells a dead run's directory from a live one's by the lock on its
/// [`Owner`] marker rather than by its name.
pub(super) fn staging_name() -> String {
    format!(".{PACKAGE}.build.{}", crate::paths::run_tag())
}

/// The marker that says a staging directory's run is still alive: its name
/// with `.owner` after it, beside it rather than inside it.
///
/// Beside, because inside is what gets renamed into the package.
pub(super) fn owner_marker(staging: &Path) -> PathBuf {
    let mut name = staging.as_os_str().to_owned();
    name.push(".owner");
    PathBuf::from(name)
}

/// A staging directory's claim to be alive: an exclusive lock on its marker.
///
/// **Asking the operating system rather than the clock.** Asking whether a
/// process id is alive is `kill(pid, 0)`, and this crate forbids `unsafe`.
/// The publish lock in [`lock`] once judged a holder dead by the age of a file
/// instead, and since 2026-09-24 asks the kernel the way this marker does.
/// Staging could never use age: a live run's directory goes minutes without its
/// modification time moving — longest while it waits up to ten minutes for
/// someone else's publication — and a guess wrong in that direction deletes
/// a package being built. A lock held by an open file answers the actual
/// question, safely: the kernel releases it when the process ends, however
/// it ends, `SIGKILL` included, and not a moment before. Measured on this
/// machine before it was relied on — held by a live process: `WouldBlock`;
/// the same process killed: acquired; a second handle in the *same* process:
/// `WouldBlock` too, so two builds in one process cannot sweep each other.
///
/// **Why not lock the directory itself**, which also opens: between creating
/// it and locking it there is a window in which it exists and nobody holds
/// it, and a sweep landing there deletes a live run's directory. A marker
/// claimed *before* the directory exists has no such window.
///
/// Best effort, and the fallback is the state before this existed: a
/// filesystem that refuses the marker or the lock leaves the run without one,
/// and its directory is then protected only by [`MARKERLESS_AGE`].
pub(super) struct Owner {
    path: PathBuf,
    _file: File,
}

impl Owner {
    /// How many times a claim is retried when a sweep is holding the marker.
    ///
    /// A sweep holds an unowned marker for the time it takes to unlink one
    /// file; the retries exist so that a sweep which found the marker in the
    /// instant between its creation and its lock does not leave this run
    /// markerless.
    const ATTEMPTS: u32 = 8;

    fn claim(path: PathBuf) -> Option<Self> {
        use std::os::unix::fs::MetadataExt as _;

        for _ in 0..Self::ATTEMPTS {
            let Ok(file) = File::options()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
            else {
                return None;
            };
            match file.try_lock() {
                Ok(()) => {}
                Err(fs::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(fs::TryLockError::Error(_)) => {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return None;
                }
            }
            // Locked — but a sweep may have unlinked the name between this
            // run creating the file and locking it, and a lock on a file no
            // longer at the path protects nothing. The same inode at the path
            // is the claim; anything else is another attempt.
            let held = file.metadata().map(|m| (m.dev(), m.ino()));
            let named = fs::symlink_metadata(&path).map(|m| (m.dev(), m.ino()));
            match (held, named) {
                (Ok(a), Ok(b)) if a == b => return Some(Self { path, _file: file }),
                _ => continue,
            }
        }
        None
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // Unlinked while still locked, so no sweep can find it unlocked in
        // between; the lock goes with the file right after.
        let _ = fs::remove_file(&self.path);
    }
}

/// How old a staging directory without a marker must be before it is removed.
///
/// Markerless directories are what 2.5.7 and earlier leave, and what a run on
/// a filesystem that refused the marker leaves; nothing can say whether their
/// run is alive, so their age is all there is. The modification time of a
/// staging root moves while documents are written — each group creates a
/// folder in it — and stops for the validation that follows and for the wait
/// on someone else's publication, which [`lock`] bounds at ten minutes. An
/// hour is six times that wait plus the whole write and check of the real
/// corpus, measured at a minute and a half.
pub(super) const MARKERLESS_AGE: std::time::Duration = std::time::Duration::from_secs(3600);

/// What a name in the destination is, as far as the sweep is concerned.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Leftover {
    /// `.{PACKAGE}.build` — the shared name used before 2.2.0.
    Legacy,
    /// `.{PACKAGE}.build.<pid>.<n>`.
    Staging,
    /// `.{PACKAGE}.build.<pid>.<n>.owner`.
    Marker,
}

pub(super) fn classify_leftover(name: &str) -> Option<Leftover> {
    let prefix = format!(".{PACKAGE}.build");
    let rest = name.strip_prefix(&prefix)?;
    if rest.is_empty() {
        return Some(Leftover::Legacy);
    }
    let rest = rest.strip_prefix('.')?;
    let (tag, kind) = match rest.strip_suffix(".owner") {
        Some(tag) => (tag, Leftover::Marker),
        None => (rest, Leftover::Staging),
    };
    let (pid, counter) = tag.split_once('.')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    (digits(pid) && digits(counter)).then_some(kind)
}

/// Remove the staging directories of runs that are gone, and nothing else.
///
/// Runs before a build writes anything, so the space a killed run took is
/// back before this one needs it. A directory goes only when its marker could
/// be locked, which only happens once its run has ended; a directory without
/// a marker goes only past [`MARKERLESS_AGE`]. A marker nobody holds and with
/// no directory — a run killed between claiming and creating, or between
/// publishing and unlinking — goes as well.
///
/// Symbolic links are never followed and never removed: the name pattern is
/// no guarantee of who put an entry there, and this is a deletion in the
/// reader's Downloads folder.
///
/// Nothing here fails the build. A leftover that cannot be removed costs the
/// disk space it already cost, which is no reason to refuse a package.
pub(super) fn sweep_abandoned_staging(destination: &Path) {
    let Ok(entries) = fs::read_dir(destination) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(kind) = entry.file_name().to_str().and_then(classify_leftover) else {
            continue;
        };
        // `DirEntry::file_type` does not follow symbolic links.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        match kind {
            Leftover::Staging | Leftover::Legacy if file_type.is_dir() => {
                let marker = owner_marker(&path);
                // **Метка – только обычный файл.** Именованный канал под ее
                // именем держал `open` до появления писателя, то есть без
                // конца, и сборка висла здесь, до первого документа, где
                // отмена ее уже не слышит. Метку эта программа создает только
                // файлом; все остальное меткой не считается, и каталог рядом
                // остается как тот, чей прогон спросить нельзя.
                if fs::symlink_metadata(&marker).is_ok_and(|m| !m.file_type().is_file()) {
                    continue;
                }
                match File::open(&marker) {
                    Ok(file) if kind == Leftover::Staging => {
                        if file.try_lock().is_ok() {
                            let _ = fs::remove_dir_all(&path);
                            let _ = fs::remove_file(&marker);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        let old = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.elapsed().ok())
                            .is_some_and(|age| age > MARKERLESS_AGE);
                        if old {
                            let _ = fs::remove_dir_all(&path);
                        }
                    }
                    _ => {}
                }
            }
            Leftover::Marker if file_type.is_file() => {
                // The marker names its directory: without the suffix it is
                // that name again.
                let Some(directory) = path
                    .to_str()
                    .and_then(|s| s.strip_suffix(".owner"))
                    .map(PathBuf::from)
                else {
                    continue;
                };
                // A directory beside it is the case above, handled there with
                // the directory; only a marker alone is this one's.
                if fs::symlink_metadata(&directory).is_ok() {
                    continue;
                }
                if let Ok(file) = File::open(&path) {
                    if file.try_lock().is_ok() {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
            _ => {}
        }
    }
}
