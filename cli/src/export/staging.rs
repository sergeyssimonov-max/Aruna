//! A build until it is published: the staging directory beside the
//! destination, the marker that says whose it is, the package that was already
//! there held aside, and the sweep that clears what an earlier run abandoned.
//!
//! A failed publication attempts to restore the working package. If rollback
//! fails, the saved packages remain available to the next run.

use super::{create_dir_all, remove_dir_all, PACKAGE};
use crate::error::{ArunaError, Result};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// The working package and any backup that was already present before this run.
pub(super) struct Replaced {
    target: PathBuf,
    /// Only the package moved by this run may be put back on rollback.
    aside: Option<SavedPackage>,
    /// An earlier run's backup stays under its original name until read-back succeeds.
    previous: Option<PathBuf>,
    published: bool,
    finished: bool,
}

impl Replaced {
    pub(super) fn aside(target: &Path, destination: &Path) -> Result<Self> {
        let exists = super::target_metadata(target)?.is_some();
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(PACKAGE);
        let previous = destination.join(format!(".{name}.previous"));
        let occupied = super::target_metadata(&previous)?.is_some();
        let aside = if !exists {
            None
        } else if occupied {
            let slot = RecoverySlot::reserve(destination, name)?;
            // Link to the *existing* backup manifest before displacing the current
            // package. After an interruption this distinguishes the newer slot
            // from a slot left over beside an unrelated .previous. Filesystems
            // without hard links still publish; an ambiguous recovery is manual.
            let manifest = previous.join(super::MANIFEST);
            if fs::symlink_metadata(&previous).is_ok_and(|m| m.is_dir())
                && fs::symlink_metadata(&manifest).is_ok_and(|m| m.is_file())
            {
                let _ = fs::hard_link(manifest, slot.0.join(PREDECESSOR));
            }
            if let Err(error) = fs::rename(target, slot.package()) {
                slot.remove_if_empty();
                return Err(ArunaError::io(target)(error));
            }
            Some(SavedPackage::Displaced(slot))
        } else {
            fs::rename(target, &previous).map_err(ArunaError::io(target))?;
            Some(SavedPackage::Previous(previous.clone()))
        };
        Ok(Self {
            target: target.to_owned(),
            aside,
            previous: occupied.then_some(previous),
            published: false,
            finished: false,
        })
    }

    pub(super) fn published(&mut self) {
        self.published = true;
    }

    /// All fallible work after moving the destination goes through this method,
    /// including the staging rename itself, before the publication guard is dropped.
    pub(super) fn publish<T>(
        mut self,
        staging: Staging,
        destination: &Path,
        exclusive: bool,
        read_back: impl FnOnce() -> Result<T>,
    ) -> Result<(T, Vec<PathBuf>)> {
        let result = staging.publish(&self.target).and_then(|()| {
            self.published();
            read_back()
        });
        self.finished = true;
        match result {
            Ok(value) => Ok((value, self.committed(destination, exclusive))),
            Err(publication) => match self.restore() {
                Ok(()) => Err(publication),
                Err(rollback) => Err(ArunaError::ExportRollback {
                    publication: Box::new(publication),
                    rollback: Box::new(rollback),
                }),
            },
        }
    }

    fn restore(&self) -> Result<()> {
        if self.aside.is_some() || self.published {
            // A failed metadata read gives us no authority to remove this tree.
            if super::target_metadata(&self.target)?.is_some() {
                if !self.published {
                    return Err(ArunaError::io(&self.target)(
                        std::io::ErrorKind::AlreadyExists.into(),
                    ));
                }
                remove_dir_all(&self.target)?;
            }
        }
        if let Some(aside) = &self.aside {
            let restored = match aside {
                SavedPackage::Previous(path) => fs::rename(path, &self.target),
                SavedPackage::Displaced(slot) => {
                    fs::rename(slot.package(), &self.target).map(|()| slot.remove_if_empty())
                }
            };
            restored.map_err(ArunaError::io(&self.target))?;
        }
        Ok(())
    }

    fn committed(&self, destination: &Path, exclusive: bool) -> Vec<PathBuf> {
        let mut left = Vec::new();
        let slot = match &self.aside {
            Some(SavedPackage::Displaced(slot)) => {
                slot.retire(&mut left);
                Some(slot)
            }
            Some(SavedPackage::Previous(path)) => {
                if remove_dir_all(path).is_err() {
                    left.push(path.clone());
                }
                None
            }
            None => None,
        };
        // On a disk that cannot serialize publication another run may still
        // need its recovery slot or the .previous we found on entry.
        if exclusive {
            let name = self
                .target
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(PACKAGE);
            RecoverySlot::retire_abandoned(destination, name, slot, &mut left);
        }
        if let Some(previous) = &self.previous {
            // A recovery directory can be retired, a foreign file or symlink cannot.
            if !exclusive
                || !fs::symlink_metadata(previous).is_ok_and(|m| m.is_dir())
                || remove_dir_all(previous).is_err()
            {
                left.push(previous.clone());
            }
        }
        left
    }
}

enum SavedPackage {
    Previous(PathBuf),
    Displaced(RecoverySlot),
}

impl Drop for Replaced {
    fn drop(&mut self) {
        // Ordinary error returns call restore explicitly so the caller gets both
        // failures. Unwinding still makes one best-effort attempt under the lock.
        if !self.finished {
            let _ = self.restore();
        }
    }
}

pub(super) const PREDECESSOR: &str = ".aruna-predecessor";
pub(super) const ROLLBACK_MARKER: &str = ".aruna-rollback";

/// A displaced working copy beside an older .previous. Its marker separates
/// it from abandoned staging directories.
pub(super) struct RecoverySlot(pub(super) PathBuf, pub(super) String);

impl RecoverySlot {
    fn reserve(destination: &Path, name: &str) -> Result<Self> {
        loop {
            let path = destination.join(format!("{}.rollback", staging_name()));
            match fs::create_dir(&path) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ArunaError::io(&path)(error)),
            }
            let marker = path.join(ROLLBACK_MARKER);
            if let Err(error) = File::create_new(&marker) {
                let _ = fs::remove_dir(&path);
                return Err(ArunaError::io(marker)(error));
            }
            return Ok(Self(path, name.to_owned()));
        }
    }

    fn package(&self) -> PathBuf {
        self.0.join(&self.1)
    }

    pub(super) fn follows(&self, previous: &Path) -> bool {
        let witness = self.0.join(PREDECESSOR);
        fs::symlink_metadata(previous).is_ok_and(|m| m.is_dir())
            && fs::symlink_metadata(&witness).is_ok_and(|m| m.is_file())
            && File::open(witness)
                .is_ok_and(|f| super::lock::same_file(&f, &previous.join(super::MANIFEST)))
    }

    pub(super) fn remove_if_empty(&self) {
        if self
            .0
            .parent()
            .is_some_and(|dir| self.follows(&dir.join(format!(".{}.previous", self.1))))
        {
            let _ = fs::remove_file(self.0.join(PREDECESSOR));
        }
        let _ = fs::remove_file(self.0.join(ROLLBACK_MARKER));
        let _ = fs::remove_dir(&self.0);
    }

    fn retire(&self, left: &mut Vec<PathBuf>) {
        let package = self.package();
        match fs::remove_dir_all(&package) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => {
                left.push(package);
                return;
            }
        }
        // Only the marked package is ours. An unrelated sibling stays in place.
        self.remove_if_empty();
    }

    fn retire_abandoned(
        destination: &Path,
        name_of_package: &str,
        current: Option<&Self>,
        left: &mut Vec<PathBuf>,
    ) {
        let Ok(entries) = fs::read_dir(destination) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let kind = name
                .to_str()
                .and_then(|s| s.strip_suffix(".rollback"))
                .and_then(classify_leftover);
            if kind != Some(Leftover::Staging) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let slot = Self(entry.path(), name_of_package.to_owned());
            if current.is_some_and(|own| own.0 == slot.0) {
                continue;
            }
            if !fs::symlink_metadata(slot.0.join(ROLLBACK_MARKER))
                .is_ok_and(|m| m.is_file() && m.len() == 0)
            {
                continue;
            }
            // A slot left by a dead process only ever held a working package
            // that passed the destination check. Keep anything unrecognizable.
            let package = slot.package();
            if !fs::symlink_metadata(&package).is_ok_and(|meta| meta.is_dir()) {
                continue;
            }
            if super::validate::check_destination(&package).is_err() {
                left.push(package);
                continue;
            }
            slot.retire(left);
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
            remove_dir_all(&path)?;
        }
        create_dir_all(&path)?;
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
            if super::lock::same_file(&file, &path) {
                return Some(Self { path, _file: file });
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

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn publishing_an_edition_preserves_another_editions_recovery_copies() {
        let dir = tempfile::tempdir().expect("destination");
        let out = dir.path();
        let target = out.join("TLHdig_Beta_0.4");
        let previous = out.join(".TLHdig_Beta_0.3.previous");
        let slot = RecoverySlot::reserve(out, "TLHdig_Beta_0.3").expect("slot");
        for path in [&previous, &slot.package()] {
            fs::create_dir(path).expect("recovery copy");
            fs::write(path.join("TLHdig_Beta_0.3.html"), b"saved").expect("inventory");
        }
        let staging = Staging::fresh(out.join(staging_name())).expect("staging");
        Replaced::aside(&target, out)
            .expect("reserve")
            .publish(staging, out, true, || Ok(()))
            .expect("publish");
        for path in [&previous, &slot.package()] {
            assert_eq!(
                fs::read(path.join("TLHdig_Beta_0.3.html")).expect("preserved"),
                b"saved"
            );
        }
    }

    #[test]
    fn a_lockless_retry_retires_only_its_own_slot() {
        let dir = tempfile::tempdir().expect("destination");
        let out = dir.path();
        let target = out.join(PACKAGE);
        let previous = out.join(format!(".{PACKAGE}.previous"));
        let other = RecoverySlot::reserve(out, PACKAGE).expect("another publisher's slot");
        let unmarked = out.join(format!(".{PACKAGE}.build.99999.1.rollback"));
        for root in [
            &previous,
            &target,
            &other.package(),
            &unmarked.join(PACKAGE),
        ] {
            fs::create_dir_all(root).expect("directory");
            fs::write(root.join(crate::paths::OUTPUT_FILE_NAME), b"keep").expect("inventory");
        }
        let publish = |exclusive| {
            let staging = Staging::fresh(out.join(staging_name())).expect("fresh");
            Replaced::aside(&target, out)
                .expect("move aside")
                .publish(staging, out, exclusive, || Ok(()))
                .expect("publish")
                .1
        };
        assert_eq!(publish(false).as_slice(), std::slice::from_ref(&previous));
        for root in [&previous, &other.package()] {
            assert_eq!(
                fs::read(root.join(crate::paths::OUTPUT_FILE_NAME)).expect("preserved"),
                b"keep"
            );
        }
        assert!(publish(true).is_empty());
        assert!(!previous.exists() && !other.0.exists());
        assert_eq!(
            fs::read(unmarked.join(PACKAGE).join(crate::paths::OUTPUT_FILE_NAME))
                .expect("unmarked"),
            b"keep"
        );
    }

    #[test]
    fn a_failed_staging_rename_does_not_remove_an_unpublished_occupant() {
        let dir = tempfile::tempdir().expect("destination");
        let out = dir.path();
        let target = out.join(PACKAGE);
        fs::create_dir(&target).expect("target");
        fs::write(target.join("data"), b"original").expect("file");
        let staging = Staging::fresh(out.join(staging_name())).expect("fresh");
        let held = Replaced::aside(&target, out).expect("move aside");
        fs::create_dir(&target).expect("foreign occupant");
        fs::write(target.join("data"), b"other").expect("file");
        let failure = held
            .publish(staging, out, true, || -> Result<()> {
                panic!("the failed staging rename must stop before read-back")
            })
            .expect_err("occupied target");
        let ArunaError::ExportRollback {
            publication,
            rollback,
        } = failure
        else {
            panic!("{failure}")
        };
        assert!(matches!(*publication, ArunaError::Io { path, .. } if path == target));
        assert!(
            matches!(*rollback, ArunaError::Io { path, source } if path == target && source.kind() == std::io::ErrorKind::AlreadyExists)
        );
        assert_eq!(fs::read(target.join("data")).expect("occupant"), b"other");
        assert_eq!(
            fs::read(out.join(format!(".{PACKAGE}.previous")).join("data")).expect("original"),
            b"original"
        );
    }
}
