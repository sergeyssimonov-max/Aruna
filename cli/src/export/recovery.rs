//! Reader-side recovery of a publication interrupted between directory renames.
use super::lock::Publication;
use super::staging::{classify_leftover, Leftover, RecoverySlot, ROLLBACK_MARKER};
use super::{target_metadata, validate, PdfState, Placed, MANIFEST};
use crate::editions::{is_package_name, library_paths};
use crate::error::{ArunaError, Result};
use crate::job::Job;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn previous_name(name: &str) -> Option<&str> {
    name.strip_prefix('.')?
        .strip_suffix(".previous")
        .filter(|s| is_package_name(s))
}

fn is_slot(name: &str) -> bool {
    name.strip_suffix(".rollback").and_then(classify_leftover) == Some(Leftover::Staging)
}

/// Ordinary libraries (including read-only ones) need no lock file or writes.
/// When publication has left evidence, wait and make the decision again under
/// the publisher's own lock. Never run this from inside an active publication.
pub fn reader_paths(parent: &Path) -> Result<(PathBuf, PathBuf)> {
    reader_paths_with(parent, &Job::unattended())
}

fn reader_paths_with(parent: &Path, job: &Job<'_>) -> Result<(PathBuf, PathBuf)> {
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(library_paths(parent)),
        Err(e) => return Err(ArunaError::io(parent)(e)),
    };
    let lock = super::lock::lock_path(parent);
    for entry in entries {
        let entry = entry.map_err(ArunaError::io(parent))?;
        let name = entry.file_name();
        if entry.path() == lock
            || name
                .to_str()
                .is_some_and(|n| previous_name(n).is_some() || is_slot(n))
        {
            if !fs::symlink_metadata(parent)
                .map_err(ArunaError::io(parent))?
                .is_dir()
            {
                return Err(invalid(
                    parent,
                    "для восстановления нужна обычная папка библиотеки",
                ));
            }
            let guard = Publication::acquire(parent, job)?;
            let copies = backups(parent)?;
            for (name, mut saved) in copies {
                saved.sort();
                let target = parent.join(&name);
                recover(&target, &name, &saved, guard.exclusive).map_err(|err| {
                    let paths = saved
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    invalid(
                        &target,
                        format!(
                            "{}. Резервные копии: {paths}",
                            match &err {
                                ArunaError::ExportInvalid { root, first, .. } =>
                                    format!("{} – {first}", root.display()),
                                _ => err.to_string(),
                            }
                        ),
                    )
                })?;
            }
            return Ok(library_paths(parent));
        }
    }
    Ok(library_paths(parent))
}

fn backups(parent: &Path) -> Result<BTreeMap<String, Vec<PathBuf>>> {
    let mut found = BTreeMap::<String, Vec<PathBuf>>::new();
    for entry in fs::read_dir(parent).map_err(ArunaError::io(parent))? {
        let entry = entry.map_err(ArunaError::io(parent))?;
        let filename = entry.file_name();
        let Some(name) = filename.to_str() else {
            continue;
        };
        let path = entry.path();
        if let Some(package) = previous_name(name) {
            found.entry(package.into()).or_default().push(path);
        } else if is_slot(name)
            && entry.file_type().map_err(ArunaError::io(&path))?.is_dir()
            && target_metadata(&path.join(ROLLBACK_MARKER))?
                .is_some_and(|m| m.is_file() && m.len() == 0)
        {
            // Replaced::aside moves exactly one working package into a slot.
            let mut single = None;
            for child in fs::read_dir(&path).map_err(ArunaError::io(&path))? {
                let child = child.map_err(ArunaError::io(&path))?;
                if let Some(package) = child.file_name().to_str().filter(|s| is_package_name(s)) {
                    if single.is_some() {
                        return Err(invalid(
                            &path,
                            format!(
                                "В резервной папке {} обнаружено несколько библиотек",
                                path.display()
                            ),
                        ));
                    }
                    single = Some((package.to_owned(), child.path()));
                }
            }
            if let Some((package, copy)) = single {
                found.entry(package).or_default().push(copy);
            }
        }
    }
    Ok(found)
}

fn recover(target: &Path, name: &str, saved: &[PathBuf], exclusive: bool) -> Result<()> {
    if !exclusive {
        return Err(invalid(
            target,
            "не удалось получить исключительный доступ к папке",
        ));
    }
    if target_metadata(target)?.is_some() {
        // A kill after the second rename may have left a complete new edition.
        // Never move or delete an occupied working name on a reader's behalf.
        return verify_copy(target, name);
    }
    let candidate = match saved {
        [only] => only,
        [a, b] => {
            let (previous, slot) = if a.parent() == target.parent() {
                (a, b)
            } else {
                (b, a)
            };
            if previous.parent() != target.parent()
                || !slot.parent().is_some_and(|dir| {
                    RecoverySlot(dir.to_owned(), name.to_owned()).follows(previous)
                })
            {
                return Err(invalid(
                    target,
                    "не удалось определить последнюю рабочую копию",
                ));
            }
            slot
        }
        _ => {
            return Err(invalid(
                target,
                "не удалось определить последнюю рабочую копию",
            ))
        }
    };
    verify_copy(candidate, name)?;
    restore_name(candidate, target)?;
    if let Some(dir) = candidate.parent().filter(|d| Some(*d) != target.parent()) {
        RecoverySlot(dir.to_owned(), name.to_owned()).remove_if_empty();
    }
    Ok(())
}

// The directory may have been created outside Aruna during validation. Even
// an empty directory belongs to its creator. Use one no-replace operation.
fn restore_name(candidate: &Path, target: &Path) -> Result<()> {
    use rustix::fs::{renameat_with, RenameFlags, CWD};
    renameat_with(CWD, candidate, CWD, target, RenameFlags::NOREPLACE)
        .map_err(|err| ArunaError::io(target)(err.into()))
}

fn invalid(path: &Path, reason: impl Into<String>) -> ArunaError {
    ArunaError::ExportInvalid {
        root: path.to_owned(),
        count: 1,
        first: reason.into(),
    }
}

#[derive(Deserialize)]
struct SavedManifest {
    package: String,
    inventory: String,
    counts: Counts,
    pdfs: PdfCounts,
    groups: Vec<Group>,
}
#[derive(Deserialize)]
struct Counts {
    groups: usize,
    documents: usize,
}
#[derive(Deserialize)]
struct PdfCounts {
    built: usize,
    refused: usize,
}
#[derive(Deserialize)]
struct Group {
    dir: String,
    documents: Vec<Document>,
}
#[derive(Deserialize)]
struct Document {
    file: PathBuf,
    siglum: String,
    sha256: String,
    pdf: SavedPdf,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum SavedPdf {
    Built(PathBuf),
    Refused { refused: String },
}

fn regular(path: &Path) -> Result<fs::Metadata> {
    let m = fs::symlink_metadata(path).map_err(ArunaError::io(path))?;
    if !m.is_file() || m.len() == 0 {
        return Err(invalid(path, "ожидался непустой обычный файл"));
    }
    Ok(m)
}

// The manifest contains digests of XML, but only paths for PDFs. Reject a
// missing/truncated PDF without loading a whole corpus of PDF objects.
fn check_pdf(path: &Path) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    let len = regular(path)?.len();
    let mut f = fs::File::open(path).map_err(ArunaError::io(path))?;
    let mut header = [0; 5];
    f.read_exact(&mut header).map_err(ArunaError::io(path))?;
    f.seek(SeekFrom::End(-(len.min(64) as i64)))
        .map_err(ArunaError::io(path))?;
    let mut tail = Vec::new();
    f.take(64)
        .read_to_end(&mut tail)
        .map_err(ArunaError::io(path))?;
    if &header != b"%PDF-" || !tail.trim_ascii_end().ends_with(b"%%EOF") {
        return Err(invalid(path, "не удалось прочитать начало или конец PDF"));
    }
    Ok(())
}

fn verify_copy(root: &Path, name: &str) -> Result<()> {
    if !target_metadata(root)?.is_some_and(|m| m.is_dir()) {
        return Err(invalid(root, "ожидалась папка библиотеки"));
    }
    let inventory = format!("{name}.html");
    for required in super::root_files(&inventory) {
        regular(&root.join(required))?;
    }
    let path = root.join(MANIFEST);
    let json = validate::read_bounded(&path, validate::MAX_MANIFEST)?;
    let manifest: SavedManifest = serde_json::from_str(&json)
        .map_err(|_| invalid(&path, "не удалось прочитать состав библиотеки"))?;
    if manifest.package != name
        || manifest.inventory != inventory
        || manifest.counts.groups != manifest.groups.len()
    {
        return Err(invalid(
            &path,
            "название или число групп расходится с описанием библиотеки",
        ));
    }
    let edition = crate::editions::installed_named(root, name)?;
    if name != super::PACKAGE
        && !edition.map_or(name == super::LEGACY_PACKAGE, |e| {
            e.folder_rank(name).is_some()
        })
    {
        return Err(invalid(
            &path,
            "издание в манифесте расходится с именем папки",
        ));
    }
    let mut placed = Vec::new();
    let mut pdfs = Vec::new();
    let mut groups = Vec::new();
    for group in manifest.groups {
        if Path::new(&group.dir).file_name().and_then(|s| s.to_str()) != Some(&group.dir)
            || !target_metadata(&root.join(&group.dir))?.is_some_and(|m| m.is_dir())
        {
            return Err(invalid(root, "неверная папка группы"));
        }
        let dir = PathBuf::from(group.dir);
        for doc in group.documents {
            if doc.file.parent() != Some(dir.as_path())
                || doc.file.extension().is_none_or(|s| s != "xml")
            {
                return Err(invalid(&path, "неверный путь рукописи"));
            }
            let xml = root.join(&doc.file);
            if regular(&xml)?.len() > super::MAX_DOCUMENT
                || crate::sha256::sha256_file(&xml).map_err(ArunaError::io(&xml))? != doc.sha256
            {
                return Err(invalid(
                    &xml,
                    "контрольная сумма рукописи расходится с манифестом",
                ));
            }
            pdfs.push(match doc.pdf {
                SavedPdf::Built(p) if p == super::pdf_path(&doc.file) => {
                    let pdf = root.join(&p);
                    check_pdf(&pdf)?;
                    PdfState::Built(p)
                }
                SavedPdf::Built(_) => return Err(invalid(&path, "неверный путь PDF")),
                SavedPdf::Refused { refused } => PdfState::Refused(refused),
            });
            placed.push(Placed {
                relative: doc.file,
                label: doc.siglum,
            });
        }
        groups.push(dir);
    }
    if placed.is_empty() || placed.len() != manifest.counts.documents {
        return Err(invalid(&path, "число рукописей расходится с манифестом"));
    }
    let built = pdfs.iter().filter(|state| state.built().is_some()).count();
    if manifest.pdfs.built != built || manifest.pdfs.refused != pdfs.len() - built {
        return Err(invalid(&path, "число PDF расходится с манифестом"));
    }
    validate::validate_layout(root, &placed, &pdfs, &inventory, groups.into_iter(), true)?;
    Ok(())
}

#[cfg(test)]
mod tests;
