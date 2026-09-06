//! What a package on disk says about itself, counted from the package.
//!
//! The layout this reads is the one [`super`] writes: a directory per CTH
//! group, the group's manuscripts inside it, and the inventory and the manifest
//! at the root ([`super::is_root_file`]). Reading it back therefore belongs
//! here, beside the code that laid it out, for the same reason
//! [`super::is_root_file`] exists at all — the rule was written twice once, and
//! the second copy is the one that goes stale.
//!
//! It was written twice again. Until 2026-09-06 the desktop shell walked the
//! package itself, in `src-tauri/src/lib.rs`, and derived the layout a second
//! time: group means subdirectory, manuscript means `.xml` inside one. Its own
//! comment recorded where the two would part — a siglum with a slash in the
//! label — and specification 4.9.6 forbids the arrangement by name: the shell
//! opens no path of its own to corpus data. This module is that path, and the
//! shell now calls it.
//!
//! **The manifest is read by whoever reads it.** This module counts what is on
//! disk and nothing else. The package's own manifest answers the same question
//! more cheaply and more fully — it is the export's own tally, written as the
//! files were laid down — so a caller asks it first and comes here when there
//! is no manifest, or none that carries counts. What both paths must agree on
//! is the arithmetic over group sizes, and that is [`spread`], which is here
//! for both to call.

use std::path::Path;

/// The largest group in a package, by the name the package gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSize {
    /// The group's label, as the package names it.
    pub label: String,
    /// How many manuscripts it holds.
    pub fragments: usize,
}

/// How the corpus is spread across its groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spread {
    /// The biggest group, or `None` when there are no groups at all.
    pub largest: Option<GroupSize>,
    /// How many groups hold exactly one manuscript.
    pub singletons: usize,
    /// How many manuscripts carry no CTH number.
    pub without_cth: usize,
}

/// What a walk of the package found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCounts {
    /// Manuscripts, across every group.
    pub documents: usize,
    /// Groups, meaning directories.
    pub groups: usize,
    /// The breakdown of the same walk.
    pub spread: Spread,
}

/// Why a package could not be counted.
#[derive(Debug, thiserror::Error)]
pub enum CountError {
    /// The path is not a directory — which is not a statement about a corpus.
    #[error("no package at that path")]
    NotAPackage,
    /// The directory is there and could not be read.
    #[error("the package could not be read: {0}")]
    Read(#[source] std::io::Error),
}

/// The breakdown, from groups named and sized.
///
/// One function for however many sources there are, because the questions asked
/// of a list of groups do not depend on where the list came from. A second copy
/// of this arithmetic would diverge from the first exactly when the two sources'
/// numbers were put side by side — which is the one time anyone looks.
///
/// The largest is decided by a strict comparison, so a tie leaves the first
/// group seen; the order is the one the caller enumerates in.
pub fn spread<I>(groups: I) -> Spread
where
    I: IntoIterator<Item = (String, usize)>,
{
    let mut singletons = 0;
    let mut without_cth = 0;
    let mut largest: Option<GroupSize> = None;

    for (label, fragments) in groups {
        if fragments == 1 {
            singletons += 1;
        }
        if label == crate::parse::MISSING {
            without_cth += fragments;
        }
        if largest
            .as_ref()
            .is_none_or(|biggest| fragments > biggest.fragments)
        {
            largest = Some(GroupSize { label, fragments });
        }
    }

    Spread {
        largest,
        singletons,
        without_cth,
    }
}

/// Count a laid-out package by walking it: groups are directories, manuscripts
/// are the `.xml` files inside them.
///
/// The two files at the root — the inventory and the manifest — are not counted
/// as groups because they are not directories, which is the same reason they
/// never were.
///
/// **The archive filter is deliberately not reused here.**
/// [`crate::parse::is_manuscript_xml`] refuses hidden files and `__MACOSX`
/// entries, because an archive from Zenodo carries a stranger's junk. A package
/// is this program's own output; a dotted `.xml` inside a group would be a file
/// someone put there, and calling it something other than a document is a
/// decision this function has never made and does not start making now.
///
/// The label is the directory's name rather than the group's own label. Export
/// derives the first from the second through [`super::dir_component`], and for
/// a group whose label needs no escaping they are the same string. Where they
/// part — a slash inside a label — a walk of the disk can only report what the
/// disk is called, and does.
pub fn count_package(package: &Path) -> Result<PackageCounts, CountError> {
    if !package.is_dir() {
        return Err(CountError::NotAPackage);
    }

    let mut sizes: Vec<(String, usize)> = Vec::new();
    let mut documents = 0;
    for group in std::fs::read_dir(package).map_err(CountError::Read)? {
        let group = group.map_err(CountError::Read)?;
        if !group.file_type().map_err(CountError::Read)?.is_dir() {
            continue;
        }
        let mut fragments = 0;
        for document in std::fs::read_dir(group.path()).map_err(CountError::Read)? {
            let document = document.map_err(CountError::Read)?;
            let path = document.path();
            let is_xml = path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"));
            if is_xml {
                fragments += 1;
            }
        }
        documents += fragments;
        sizes.push((group.file_name().to_string_lossy().into_owned(), fragments));
    }

    Ok(PackageCounts {
        documents,
        groups: sizes.len(),
        spread: spread(sizes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A package of `groups` directories holding `each` documents apiece.
    ///
    /// A non-XML file sits beside the documents: the walk counts manuscripts,
    /// not directory entries.
    fn package(root: &Path, groups: usize, each: usize) {
        for group in 0..groups {
            let dir = root.join(format!("CTH {group}"));
            fs::create_dir_all(&dir).expect("group directory");
            for document in 0..each {
                fs::write(dir.join(format!("KBo {document}.xml")), b"<doc/>").expect("document");
            }
            fs::write(dir.join("README.txt"), b"not a manuscript").expect("a file that is not one");
        }
    }

    #[test]
    fn a_package_is_counted_by_walking_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        package(dir.path(), 4, 5);

        let counts = count_package(dir.path()).expect("counted");

        assert_eq!(counts.documents, 20);
        assert_eq!(counts.groups, 4);
        assert_eq!(counts.spread.singletons, 0);
        assert_eq!(counts.spread.without_cth, 0);

        // All four groups are the same size, so the size is what is checked;
        // the name only for coming from the package at all, because the order
        // a filesystem hands back directories in is promised by no one.
        let largest = counts.spread.largest.expect("the package has groups");
        assert_eq!(largest.fragments, 5);
        assert!(
            largest.label.starts_with("CTH "),
            "the largest group is not one of the package's: {}",
            largest.label
        );
    }

    /// The files at the package's root are not groups.
    #[test]
    fn the_inventory_and_the_manifest_are_not_groups() {
        let dir = tempfile::tempdir().expect("tempdir");
        package(dir.path(), 2, 3);
        fs::write(dir.path().join(super::super::MANIFEST), b"{}").expect("manifest");
        fs::write(
            dir.path().join(format!("{}.html", super::super::PACKAGE)),
            b"<html>",
        )
        .expect("inventory");

        let counts = count_package(dir.path()).expect("counted");

        assert_eq!(counts.groups, 2);
        assert_eq!(counts.documents, 6);
    }

    /// The group with no CTH is recognised by the label the parse gives it,
    /// not by a dash written out here a second time.
    #[test]
    fn a_group_without_a_cth_is_recognised_by_the_parses_own_label() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (group, documents) in [(crate::parse::MISSING, 3), ("CTH 1", 1)] {
            let path = dir.path().join(group);
            fs::create_dir_all(&path).expect("group directory");
            for document in 0..documents {
                fs::write(path.join(format!("KBo {document}.xml")), b"<doc/>").expect("document");
            }
        }

        let counts = count_package(dir.path()).expect("counted");

        assert_eq!(counts.spread.without_cth, 3);
        assert_eq!(counts.spread.singletons, 1, "that is CTH 1, not the other");
        assert_eq!(
            counts.spread.largest,
            Some(GroupSize {
                label: crate::parse::MISSING.to_owned(),
                fragments: 3,
            })
        );
    }

    /// A path that is not a package is a named failure rather than a zero.
    ///
    /// Nought manuscripts is a statement about a corpus, and a window would
    /// show it as a number. The absence of a package is not such a statement.
    #[test]
    fn a_path_that_is_not_a_package_is_a_named_failure() {
        let dir = tempfile::tempdir().expect("tempdir");

        let failure = count_package(&dir.path().join("nothing-here")).expect_err("no package");

        assert!(matches!(failure, CountError::NotAPackage));
    }

    /// An empty package counts to nothing and names no largest group.
    #[test]
    fn an_empty_package_has_no_largest_group() {
        let dir = tempfile::tempdir().expect("tempdir");

        let counts = count_package(dir.path()).expect("counted");

        assert_eq!(counts.documents, 0);
        assert_eq!(counts.groups, 0);
        assert_eq!(counts.spread, Spread::default());
    }

    /// The breakdown does not depend on where the list of groups came from,
    /// which is the whole reason it is one function.
    #[test]
    fn the_breakdown_reads_a_list_of_groups_whatever_wrote_it() {
        let listed = spread([
            ("CTH 832".to_owned(), 6),
            ("CTH 1".to_owned(), 1),
            (crate::parse::MISSING.to_owned(), 2),
        ]);

        assert_eq!(
            listed,
            Spread {
                largest: Some(GroupSize {
                    label: "CTH 832".to_owned(),
                    fragments: 6,
                }),
                singletons: 1,
                without_cth: 2,
            }
        );
    }

    /// A tie leaves the first group seen, in the order the caller gives.
    #[test]
    fn a_tie_leaves_the_first_group_seen() {
        let listed = spread([("CTH 1".to_owned(), 4), ("CTH 2".to_owned(), 4)]);

        assert_eq!(
            listed.largest.expect("there are groups").label,
            "CTH 1",
            "a tie must not move the answer"
        );
    }
}
