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

use super::manifest;
use super::naming::{dir_component, pdf_path, resolve};
use super::{inventory, Placed, MANIFEST, PACKAGE};
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

/// The most a manifest may be. The real one is 8.3 MiB; the same reasoning as
/// [`MAX_INVENTORY`], and the same defence against being pointed at a file this
/// program did not write.
const MAX_MANIFEST: u64 = 64 * 1024 * 1024;

/// Read the inventory, refusing one too large to be one.
///
/// `read_to_string` on a path from outside is an unbounded read, and the
/// validator is meant to be pointed at directories this program did not write.
fn read_inventory(path: &Path) -> Result<String> {
    read_bounded(path, MAX_INVENTORY)
}

/// Read a text file no larger than `limit`, or say which file failed and why.
///
/// **Refusing is the point, and it used to truncate instead.** `take(limit)`
/// stops reading and returns what it has, so a file one byte over the cap came
/// back as its first `limit` bytes with nothing to say it was cut — and the
/// caller then validated a document that does not exist on disk. An inventory
/// truncated mid-`<a>` loses links it has, which is a validation failure with a
/// false explanation; a manifest truncated mid-JSON parses as fewer entries,
/// which is the same. Both doc comments here already promised a refusal.
///
/// `limit + 1` is the whole of the fix: read one byte past the cap, and having
/// got it is proof the file is over.
fn read_bounded(path: &Path, limit: u64) -> Result<String> {
    use std::io::Read as _;

    let file = fs::File::open(path).map_err(|source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut text = String::new();
    file.take(limit.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|source| ArunaError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if text.len() as u64 > limit {
        return Err(ArunaError::ExportInvalid {
            root: path.to_path_buf(),
            count: 1,
            first: format!(
                "{} is larger than the {limit} byte limit for this file, so it is not one this program wrote",
                path.display()
            ),
        });
    }
    Ok(text)
}

/// What a clean package contains, counted while checking it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Validation {
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

    for href in inventory::hrefs(&html) {
        let Some(relative) = resolve(href) else {
            errors.push(format!(
                "link is not a relative path inside the package: {href}"
            ));
            continue;
        };
        let target = root.join(&relative);

        // Every link in this document names an XML file. Group headings carried
        // links to a page inside each CTH folder until 2026-08-23; that page is
        // no longer written and nothing may link to one, so anything that is not
        // a fragment is a fault rather than a second kind of link.
        if href.ends_with(".xml") {
            counts.fragment_links += 1;
            if target.is_file() {
                linked_files.insert(relative);
            } else {
                errors.push(format!("fragment link points at nothing: {href}"));
            }
        } else {
            errors.push(format!(
                "the inventory links something that is not a fragment: {href}"
            ));
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

    // The manifest describes the same package, and is checked against the same
    // model rather than against the inventory: two documents agreeing with each
    // other prove nothing if both were written from a model that was wrong.
    check_manifest(root, placed, &expected, &mut errors);

    // Groups: one directory each, and each one there. Nothing links a folder any
    // more, so this is checked against the filesystem rather than against the
    // links — a group whose folder never appeared would otherwise be reported
    // only as its documents going missing one by one.
    let groups: HashSet<PathBuf> = group_runs(records)
        .map(|run| PathBuf::from(dir_component(group_label(&run[0]))))
        .collect();
    for group in &groups {
        if !root.join(group).is_dir() {
            errors.push(format!("group without a folder: {}", group.display()));
        }
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

/// The manifest must name exactly the documents the package holds.
///
/// Read back from the file rather than from the string that was written, for
/// the same reason the inventory is: what a converter opens is the file.
fn check_manifest(
    root: &Path,
    placed: &[Placed],
    expected: &HashSet<PathBuf>,
    errors: &mut Vec<String>,
) {
    let path = root.join(MANIFEST);
    let json = match read_bounded(&path, MAX_MANIFEST) {
        Ok(json) => json,
        Err(err) => {
            errors.push(format!("the manifest cannot be read: {err}"));
            return;
        }
    };

    // Both directions for both names. Counting them and checking they are
    // distinct is not the same question: a manifest whose PDF names are all
    // wrong but all different passed that, which is the manifest agreeing with
    // itself rather than with the package.
    for (key, expected) in [
        ("file", expected.clone()),
        (
            "pdf",
            placed.iter().map(|p| pdf_path(&p.relative)).collect(),
        ),
    ] {
        let named: HashSet<PathBuf> = manifest::values_of(&json, key)
            .into_iter()
            .map(PathBuf::from)
            .collect();
        for missing in expected.difference(&named) {
            errors.push(format!(
                "the manifest has no {key} entry for {}",
                missing.display()
            ));
        }
        for extra in named.difference(&expected) {
            errors.push(format!(
                "the manifest has a {key} entry for {}, which is not in the package",
                extra.display()
            ));
        }
        if named.len() != placed.len() {
            errors.push(format!(
                "the manifest lists {} distinct {key} names for {} documents",
                named.len(),
                placed.len()
            ));
        }
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
    let at_root = dir == root;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        // The inventory and the manifest are part of the package by contract, at
        // the root and nowhere else. Inside a CTH folder there is nothing but
        // XML: the `index.html` that used to open each folder is no longer
        // written, so one appearing here is either a leftover from an older
        // export or a regression, and both are worth failing on.
        let belongs = at_root && super::is_root_file(&name);
        if path.is_dir() {
            walk(root, &path, depth - 1, files, errors);
        } else if belongs {
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
    let refuse = |reason: String| {
        Err(ArunaError::ExportDestination {
            path: root.to_path_buf(),
            reason,
        })
    };

    // `symlink_metadata`, not `exists`: `exists` follows links, so it answers
    // about the target and says "nothing there" for a link that dangles. Either
    // answer is the wrong one to act on — what gets renamed over, and what a
    // recursive delete would walk into, is the link.
    let Ok(meta) = fs::symlink_metadata(root) else {
        return Ok(());
    };
    if meta.file_type().is_symlink() {
        return refuse(
            "it is a symbolic link, and whatever it points at is not this exporter's to replace"
                .to_string(),
        );
    }
    if !meta.is_dir() {
        return refuse("it is a file, not a package".to_string());
    }

    if !root.join(format!("{PACKAGE}.html")).is_file() {
        return refuse(format!("there is no {PACKAGE}.html in it"));
    }
    let entries = fs::read_dir(root).map_err(|source| ArunaError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Wider than the walk's rule on purpose: a group folder is ours, and a
        // `.DS_Store` is the Finder's business rather than a reason to refuse
        // to replace our own package.
        let ours = super::is_root_file(&name)
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
        // The manifest is part of a package too, and the validator now says so.
        fs::write(
            dir.join(MANIFEST),
            manifest::render_manifest(
                &records,
                &placed,
                "test",
                "0",
                &Default::default(),
                &Default::default(),
            ),
        )
        .expect("manifest");
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
    fn a_package_without_a_manifest_is_not_a_package() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);
        fs::remove_file(dir.path().join(MANIFEST)).expect("remove");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("refused");
        assert!(format!("{err}").contains("manifest"), "{err}");
    }

    #[test]
    fn a_manifest_that_describes_a_different_package_fails() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);

        // The one failure the inventory cannot catch: the manifest promising a
        // document nobody placed, and omitting one that is there.
        let path = dir.path().join(MANIFEST);
        let json = fs::read_to_string(&path).expect("read");
        let first = placed[0].relative.to_string_lossy().to_string();
        fs::write(&path, json.replace(&first, "CTH 5/invented.xml")).expect("write");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("refused");
        let text = format!("{err}");
        assert!(text.contains("invented.xml"), "{text}");
        assert!(text.contains(&first), "{text}");
    }

    #[test]
    fn a_manifest_giving_two_documents_one_pdf_fails() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);

        let path = dir.path().join(MANIFEST);
        let json = fs::read_to_string(&path).expect("read");
        let second = pdf_path(&placed[1].relative).to_string_lossy().to_string();
        let first = pdf_path(&placed[0].relative).to_string_lossy().to_string();
        fs::write(&path, json.replace(&second, &first)).expect("write");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("refused");
        let text = format!("{err}");
        assert!(text.contains("pdf"), "{text}");
        assert!(
            text.contains(&second),
            "the name that went missing is the one to say: {text}"
        );
    }

    /// `index.html` is refused wherever it appears — this is the guard on the
    /// decision, not a tidiness check.
    ///
    /// The exporter wrote one into every CTH folder until 2026-08-23. Re-adding
    /// that would be a silent return of a feature that was deliberately given
    /// up, so a package carrying one is not a package this program built.
    #[test]
    fn an_index_page_is_no_longer_part_of_a_package_anywhere() {
        let fragments = sample();
        for at in ["", "CTH 5"] {
            let dir = tempdir().expect("tempdir");
            let placed = package(dir.path(), &fragments);
            let path = dir.path().join(at).join("index.html");
            fs::write(&path, b"<html/>").expect("write");

            let err = validate(dir.path(), &records(&fragments), &placed).expect_err("refused");
            assert!(
                format!("{err}").contains("index.html"),
                "an index.html in {at:?} was accepted: {err}"
            );
        }
    }

    #[test]
    fn a_second_manifest_inside_a_group_does_not_pass_either() {
        let dir = tempdir().expect("tempdir");
        let fragments = sample();
        let placed = package(dir.path(), &fragments);
        fs::write(dir.path().join("CTH 5").join(MANIFEST), b"{}").expect("write");

        let err = validate(dir.path(), &records(&fragments), &placed).expect_err("refused");
        assert!(format!("{err}").contains(MANIFEST), "{err}");
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
        // A normal inventory arrives whole.
        assert_eq!(read_inventory(&path).expect("read"), "<html>");

        // And the cap itself, at the only two sizes where it can be wrong.
        // This used to say that proving the other half would mean writing
        // 256 MiB — it does not: the limit is an argument, so a six-byte file
        // and a limit of six answer the same question for a fraction of a
        // millisecond. Truncation at exactly the cap is what `take` did and what
        // the caller could not detect.
        assert_eq!(read_bounded(&path, 6).expect("exactly the cap"), "<html>");
        let over = read_bounded(&path, 5).expect_err("one byte over the cap");
        assert!(
            matches!(over, ArunaError::ExportInvalid { count: 1, .. }),
            "an oversized file is refused, not truncated: {over}"
        );
    }
}
