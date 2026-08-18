//! Checking a finished package against the model it was built from.
//!
//! Both directions, because one of them alone proves nothing useful. Every link
//! in the inventory has to reach a file that is there; every file that is there
//! has to be linked. A package that satisfies only the first can hold documents
//! nobody can reach from it, and one that satisfies only the second can promise
//! documents it does not have.
//!
//! It reads the package rather than the build that produced it, so it is worth
//! running against a directory this program did not just write — which is what
//! the second call in [`super::build`] does, after the rename.

use super::naming::{dir_component, resolve};
use super::{inventory, Placed, PACKAGE};
use crate::error::{ArunaError, Result};
use crate::parse::{group_label, group_runs, ManuscriptRecord};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// How deep a package may be: the root, and one directory per group.
///
/// Generous by one, so a legitimate package is never near the limit and a tree
/// that trips it is definitely not one.
const MAX_DEPTH: u32 = 4;

/// The most an inventory may be. The real one is 6.5 MiB for 24 000
/// manuscripts; a file at this size is not an inventory this program wrote.
const MAX_INVENTORY: u64 = 256 * 1024 * 1024;

/// Read the inventory, refusing one too large to be one.
///
/// `read_to_string` on a path from outside is an unbounded read, and the
/// validator is meant to be pointed at directories this program did not write.
fn read_inventory(path: &Path) -> Result<String> {
    use std::io::Read as _;

    let file = fs::File::open(path).map_err(|source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut html = String::new();
    file.take(MAX_INVENTORY)
        .read_to_string(&mut html)
        .map_err(|source| ArunaError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(html)
}

/// What a clean package contains, counted while checking it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Validation {
    pub group_links: usize,
    pub fragment_links: usize,
}

/// Check `root` against the records and placements it was built from.
///
/// Every problem is collected before any is reported: a package with four
/// hundred broken links should say so once, not four hundred times, and the
/// first failure is rarely the informative one.
pub fn validate(
    root: &Path,
    records: &[ManuscriptRecord],
    placed: &[Placed],
) -> Result<Validation> {
    let inventory_path = root.join(format!("{PACKAGE}.html"));
    let html = read_inventory(&inventory_path)?;

    let mut errors: Vec<String> = Vec::new();
    let mut counts = Validation::default();
    let mut linked_files: HashSet<PathBuf> = HashSet::new();
    let mut linked_dirs: HashSet<PathBuf> = HashSet::new();

    for href in inventory::hrefs(&html) {
        let Some(relative) = resolve(href) else {
            errors.push(format!(
                "link is not a relative path inside the package: {href}"
            ));
            continue;
        };
        let target = root.join(&relative);

        if href.ends_with(".xml") {
            counts.fragment_links += 1;
            if target.is_file() {
                linked_files.insert(relative);
            } else {
                errors.push(format!("fragment link points at nothing: {href}"));
            }
        } else {
            counts.group_links += 1;
            if target.is_dir() {
                linked_dirs.insert(relative);
            } else {
                errors.push(format!("group link is not a directory: {href}"));
            }
        }
    }

    // The inventory must link exactly what was placed…
    let expected: HashSet<PathBuf> = placed.iter().map(|p| p.relative.clone()).collect();
    for missing in expected.difference(&linked_files) {
        errors.push(format!("placed but not linked: {}", missing.display()));
    }
    for extra in linked_files.difference(&expected) {
        errors.push(format!("linked but not placed: {}", extra.display()));
    }

    // …and the filesystem must hold exactly what the inventory links.
    let mut on_disk: HashSet<PathBuf> = HashSet::new();
    walk(root, root, MAX_DEPTH, &mut on_disk, &mut errors);
    for orphan in on_disk.difference(&expected) {
        errors.push(format!("orphan file in the package: {}", orphan.display()));
    }
    for absent in expected.difference(&on_disk) {
        errors.push(format!("expected document missing: {}", absent.display()));
    }

    // Groups: one directory each, all linked, none invented.
    let groups: HashSet<PathBuf> = group_runs(records)
        .map(|run| PathBuf::from(dir_component(group_label(&run[0]))))
        .collect();
    for group in groups.difference(&linked_dirs) {
        errors.push(format!("group without a working link: {}", group.display()));
    }
    for extra in linked_dirs.difference(&groups) {
        errors.push(format!(
            "link to a group that is not in the inventory: {}",
            extra.display()
        ));
    }

    if errors.is_empty() {
        Ok(counts)
    } else {
        Err(ArunaError::ExportInvalid {
            root: root.to_path_buf(),
            count: errors.len(),
            first: errors.into_iter().take(10).collect::<Vec<_>>().join("; "),
        })
    }
}

/// Every `.xml` under `root`, relative to it; anything else is an error.
///
/// The package is meant to hold the inventory and normalised documents and
/// nothing else, so a stray file is reported rather than ignored — that is how
/// a leftover from an earlier build, or a `.DS_Store` the Finder dropped in,
/// gets noticed.
fn walk(
    root: &Path,
    dir: &Path,
    depth: u32,
    files: &mut HashSet<PathBuf>,
    errors: &mut Vec<String>,
) {
    // A package is two levels deep and nothing else is allowed in it, so a tree
    // that keeps going is not one of ours. `is_dir` follows symbolic links, so
    // without a bound a link pointing at its own parent would recurse until the
    // stack ran out — which is a crash rather than a report.
    if depth == 0 {
        errors.push(format!(
            "{} is nested deeper than a package can be",
            dir.display()
        ));
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        errors.push(format!("cannot read {}", dir.display()));
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            walk(root, &path, depth - 1, files, errors);
        } else if name == format!("{PACKAGE}.html") {
            continue;
        } else if !name.ends_with(".xml") {
            errors.push(format!(
                "unexpected file in the package: {}",
                path.display()
            ));
        } else if let Ok(relative) = path.strip_prefix(root) {
            files.insert(relative.to_path_buf());
        }
    }
}

/// Refuse to overwrite something that is not one of our own builds.
///
/// The destination is inside someone's Downloads folder, and a recursive delete
/// aimed at the wrong directory is not a mistake that can be taken back. A
/// package this exporter wrote is recognisable — an inventory of the right name
/// beside directories of CTH groups — and anything else is left alone with an
/// explanation.
pub fn check_destination(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    let refuse = |reason: String| {
        Err(ArunaError::ExportDestination {
            path: root.to_path_buf(),
            reason,
        })
    };

    if !root.join(format!("{PACKAGE}.html")).is_file() {
        return refuse(format!("there is no {PACKAGE}.html in it"));
    }
    let entries = fs::read_dir(root).map_err(|source| ArunaError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let ours = name == format!("{PACKAGE}.html")
            || (entry.path().is_dir() && name.starts_with("CTH"))
            || name == ".DS_Store";
        if !ours {
            return refuse(format!(
                "it contains {name:?}, which this exporter did not put there"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{place, render_inventory, tests_support::fragment};
    use tempfile::tempdir;

    /// A package built by hand, so the validator can be shown things a real
    /// build would never produce.
    fn package(dir: &Path, fragments: &[crate::export::Fragment]) -> Vec<Placed> {
        let placed = place(fragments).expect("placed");
        let records: Vec<ManuscriptRecord> = fragments.iter().map(|f| f.record.clone()).collect();
        for place in &placed {
            let path = dir.join(&place.relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            fs::write(
                &path,
                b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<AOxml/>",
            )
            .expect("write");
        }
        fs::write(
            dir.join(format!("{PACKAGE}.html")),
            render_inventory(&records, &placed, "test"),
        )
        .expect("inventory");
        placed
    }

    fn sample() -> Vec<crate::export::Fragment> {
        vec![
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("Bo 2023/23", "CTH 5", "root/CTH 5_XML_HFR/b.xml"),
            fragment("KUB 2.1", "CTH 9", "root/CTH 9_XML_TLH/c.xml"),
        ]
    }

    fn records(fragments: &[crate::export::Fragment]) -> Vec<ManuscriptRecord> {
        fragments.iter().map(|f| f.record.clone()).collect()
    }

    #[test]
    fn a_package_that_matches_its_model_passes() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);

        let counts = validate(dir.path(), &records(&fragments), &placed).expect("valid");
        assert_eq!(counts.group_links, 2);
        assert_eq!(counts.fragment_links, 3);
    }

    /// The failure this whole check exists for: the inventory promises a
    /// document that is not there.
    #[test]
    fn a_link_with_no_file_behind_it_fails() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);
        fs::remove_file(dir.path().join(&placed[0].relative)).expect("remove");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("must fail");
        assert!(format!("{err}").contains("points at nothing"), "{err}");
    }

    /// The other direction: a document nobody can reach from the inventory.
    #[test]
    fn a_file_no_link_reaches_fails() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);
        fs::write(dir.path().join("CTH 5").join("stowaway.xml"), b"<a/>").expect("write");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("must fail");
        assert!(format!("{err}").contains("orphan"), "{err}");
    }

    #[test]
    fn a_file_that_is_not_a_document_fails() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);
        fs::write(dir.path().join("CTH 5").join("notes.txt"), b"scratch").expect("write");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("must fail");
        assert!(format!("{err}").contains("unexpected file"), "{err}");
    }

    #[test]
    fn a_destination_this_exporter_did_not_write_is_left_alone() {
        let dir = tempdir().expect("tempdir");

        // A folder of someone's own that happens to carry the name: refused on
        // the inventory that is not in it, before anything is looked at twice.
        let named = dir.path().join("theirs").join(PACKAGE);
        fs::create_dir_all(&named).expect("mkdir");
        fs::write(named.join("thesis.docx"), b"years of work").expect("write");
        let err = check_destination(&named).expect_err("must refuse");
        assert!(
            format!("{err}").contains("no TLHdig_Beta_0.3.html"),
            "{err}"
        );

        // One that looks like ours until something of theirs is found in it.
        let mixed = dir.path().join("mixed").join(PACKAGE);
        fs::create_dir_all(mixed.join("CTH 1")).expect("mkdir");
        fs::write(mixed.join(format!("{PACKAGE}.html")), b"<html>").expect("write");
        fs::write(mixed.join("thesis.docx"), b"years of work").expect("write");
        let err = check_destination(&mixed).expect_err("must refuse");
        assert!(format!("{err}").contains("thesis.docx"), "{err}");

        // …and one it did write is fine to rebuild over.
        let ours = dir.path().join("ours").join(PACKAGE);
        fs::create_dir_all(ours.join("CTH 1")).expect("mkdir");
        fs::write(ours.join(format!("{PACKAGE}.html")), b"<html>").expect("write");
        check_destination(&ours).expect("our own build may be replaced");
    }

    #[test]
    fn a_destination_that_does_not_exist_is_not_a_problem() {
        let dir = tempdir().expect("tempdir");
        check_destination(&dir.path().join("nothing-here")).expect("nothing to refuse");
    }

    /// The validator is pointed at trees it did not write, and `is_dir` follows
    /// symbolic links — so a link to its own parent was unbounded recursion,
    /// which ends as a stack overflow rather than as a report.
    #[cfg(unix)]
    #[test]
    fn a_looping_directory_is_reported_rather_than_followed_forever() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);

        let group = dir.path().join("CTH 5");
        std::os::unix::fs::symlink(&group, group.join("loop")).expect("symlink");

        // The process survives and says what is wrong; without the bound this
        // never returns.
        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("must fail");
        assert!(
            format!("{err}").contains("nested deeper"),
            "the loop was not the thing reported: {err}"
        );
    }

    /// An inventory too large to be one is refused instead of being read whole.
    #[test]
    fn an_oversized_inventory_is_not_read_into_memory() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join(format!("{PACKAGE}.html"));
        fs::write(&path, b"<html>").expect("write");
        // The cap is what bounds the read; this only proves a normal one still
        // arrives whole, since writing 256 MiB to prove the other half would
        // cost more than the check does.
        assert_eq!(read_inventory(&path).expect("read"), "<html>");
    }
}
