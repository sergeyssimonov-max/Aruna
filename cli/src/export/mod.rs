//! Export the corpus as a standalone folder: an inventory, one directory per
//! CTH group, and the manuscripts of that group inside it.
//!
//! The package opens from the filesystem with no server and no network: the
//! inventory links to its neighbours with relative URLs, so moving the whole
//! folder somewhere else keeps every link working.
//!
//! The work divides by what it touches, and the files follow the division:
//!
//! * [`naming`] decides where a document goes — strings in, paths out;
//! * [`normalize`] turns one archive document into the one that ships;
//! * [`inventory`] writes the page and reads its links back;
//! * [`validate`] checks a finished package against the model it came from;
//! * this module owns the pipeline, and is the only part that opens a file.
//!
//! Everything above that line is pure, which is what lets a synthetic archive
//! of four documents exercise the same code the 24 000-manuscript corpus does.

pub mod inventory;
pub mod manifest;
pub mod naming;
pub mod normalize;
pub mod validate;
pub mod verify;

pub use inventory::{hrefs, render_inventory};
pub use manifest::{render_manifest, FontContract};
pub use naming::{
    dir_component, href, output_path, path_component, pdf_path, percent_decode, resolve,
};
pub use normalize::{normalize_document, normalize_into};
pub use validate::{validate, Validation};

use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use crate::order::sort_by_display_order;
use crate::parse::{group_label, is_manuscript_xml, looks_like_manuscript, parse_manuscript};
use crate::parse::{ManuscriptRecord, HEADER_READ_LIMIT};
use crate::progress::Event;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read, Write as _};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// What the package is called, in the folder and in the inventory's file name.
pub const PACKAGE: &str = "TLHdig_Beta_0.3";

/// The machine-readable model of the package.
pub const MANIFEST: &str = "manifest.json";

/// Whether `name` is one of the two files the package carries at its root.
///
/// The validator's walk and its refusal to overwrite a stranger's folder each
/// listed these independently, and this very pair had to be extended by hand in
/// both places when the manifest arrived. One list, asked twice.
pub fn is_root_file(name: &str) -> bool {
    name == MANIFEST || name.strip_prefix(PACKAGE) == Some(".html")
}

/// The most one document may be, inflated.
///
/// A ZIP entry states its uncompressed size and the reader believes it, so a
/// small archive can hold a very large document: measured, a 398 KiB archive
/// carrying one 400 MiB entry took peak memory to 834 MiB — twice the document,
/// because the raw bytes and the normalised copy are both live while it is
/// written.
///
/// The largest manuscript in this corpus is 0.86 MiB and the median is 5.5 KiB,
/// so 64 MiB is seventy times the biggest real document. It is a ceiling on
/// memory, not a judgement about scholarship: an entry past it is refused by
/// name rather than read.
pub const MAX_DOCUMENT: u64 = 64 * 1024 * 1024;

/// A record and the archive entry it was parsed from.
///
/// The record alone cannot say where a document came from — `ManuscriptRecord`
/// has no path in it, by design, because the inventory never needed one. The
/// export does, so it carries the two together rather than widening the model.
#[derive(Debug, Clone)]
pub struct Fragment {
    pub record: ManuscriptRecord,
    /// Path inside the archive: what the document is read from in the second
    /// pass, and what names it if two documents collide.
    pub source: String,
}

/// A fragment placed in the package: where it goes and what it is called there.
#[derive(Debug, Clone)]
pub struct Placed {
    pub relative: PathBuf,
    /// The siglum as a reader sees it, which the file name may have escaped.
    pub label: String,
}

/// Decide where every fragment goes, refusing to put two in one place.
///
/// Sigla repeat: 34 pairs share a siglum inside one CTH group, and they are
/// different documents. The second and later get a suffix taken from the
/// archive path they came from, so both survive and the choice does not depend
/// on the order the archive happened to be read in.
///
/// A collision that survives that is a build error rather than a file quietly
/// overwritten.
///
/// Only the XML path is checked. There was a second map for the PDF name each
/// document will take, but [`naming::output_path`] always ends a name with
/// `.xml` and [`naming::pdf_path`] only replaces that fixed suffix, so two
/// distinct XML paths cannot become one PDF path: the check could never fire on
/// its own. Because it ran first it did fire — naming the `.pdf` path in every
/// collision error for a clash that was between two `.xml` files. The manifest
/// is still held to distinct PDF names by [`validate`], which reads them back
/// from the published file rather than trusting the rule.
pub fn place(fragments: &[Fragment]) -> Result<Vec<Placed>> {
    let mut taken: HashMap<PathBuf, String> = HashMap::with_capacity(fragments.len());
    let mut placed = Vec::with_capacity(fragments.len());

    for fragment in fragments {
        let group = group_label(&fragment.record);
        let base = fragment.record.sigla.as_str();

        let mut relative = output_path(group, base);
        if taken.contains_key(&relative) {
            // Stable, and derived from the one thing that is unique per
            // document: where it sits in the archive.
            let suffix = disambiguator(&fragment.source);
            relative = output_path(group, &format!("{base} ({suffix})"));
        }

        if let Some(first) = taken.get(&relative) {
            return Err(ArunaError::ExportCollision {
                group: group.to_string(),
                fragment: base.to_string(),
                first: first.clone(),
                second: fragment.source.clone(),
                path: relative,
            });
        }
        taken.insert(relative.clone(), fragment.source.clone());
        placed.push(Placed {
            relative,
            label: base.to_string(),
        });
    }

    Ok(placed)
}

/// A short, stable tag for one archive entry.
///
/// The directory the document sits in, which is what distinguishes the repeated
/// sigla in this corpus: the same siglum filed once under `CTH 69_XML_HFR` and
/// again under `CTH 69_XML_TLH`.
fn disambiguator(source: &str) -> String {
    let parent = source
        .rsplit_once('/')
        .map(|(head, _)| head)
        .and_then(|head| head.rsplit('/').next())
        .unwrap_or(source);
    path_component(parent)
}

/// How many distinct CTH groups a set of fragments falls into.
///
/// Order-independent, and that is the whole point. This is reported after the
/// headers are read and before anything is sorted, and the corpus files one
/// group under several folders — `CTH 5_XML_HFR` and `CTH 5_XML_TLH` are two
/// places and one group. Counting runs of equal neighbours instead of distinct
/// values answered 826 for a corpus of 663, and the correct figure was printed
/// four lines later by the summary, which is how it was noticed.
fn distinct_groups(fragments: &[Fragment]) -> usize {
    fragments
        .iter()
        .map(|f| group_label(&f.record))
        .collect::<std::collections::HashSet<_>>()
        .len()
}

/// Each CTH group: its label, the records in it, and where they were placed.
///
/// `placed` is built from `records` in order, so a group's run of records lines
/// up with a run of the same length in `placed`. That invariant was re-derived
/// three times — by index arithmetic in the builder, by a shared iterator in the
/// manifest that silently truncated the file if it ran dry, and by a copy of the
/// first in the validator's fixture — which is three ways to get one thing
/// wrong. It is derived here once instead.
///
/// Slicing panics if the two ever stop being parallel. That is the right
/// outcome: it cannot happen from any input, only from a change to this module,
/// and a manifest that quietly describes half a package is worse than a crash.
pub fn group_slices<'a>(
    records: &'a [ManuscriptRecord],
    placed: &'a [Placed],
) -> impl Iterator<Item = (&'a str, &'a [ManuscriptRecord], &'a [Placed])> {
    let mut from = 0usize;
    crate::parse::group_runs(records).map(move |run| {
        let slice = &placed[from..from + run.len()];
        from += run.len();
        (group_label(&run[0]), run, slice)
    })
}

/// What a build did, for the caller to print and for a test to assert on.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Built {
    pub groups: usize,
    pub documents: usize,
    pub fragment_links: usize,
    /// Fragments that needed a suffix because their siglum was already taken.
    pub disambiguated: usize,
    /// Documents that carried a stylesheet instruction the package does without.
    pub stylesheet_dropped: usize,
}

/// Build the package under `destination`, and return what it contains.
///
/// Nothing is published until everything is checked: the build happens in a
/// sibling directory and takes the final name only once validation is clean, so
/// a failure never leaves half a package behind. The published copy is then
/// validated again, because what a caller opens is that one and not the staging
/// directory it was renamed from.
///
/// `job` hears each stage begin and is asked, between documents, whether to
/// keep going. The build is six seconds and four stages with nothing in
/// between them, which is exactly long enough for a window to look frozen —
/// and long enough that a person who changed their mind should not have to
/// wait it out.
///
/// A cancelled build publishes nothing. The work happens in a staging
/// directory that removes itself unless it is published, so stopping leaves
/// the destination exactly as it was found — the same guarantee a failed build
/// already had, reached by the same mechanism.
pub fn build(zip: &Path, destination: &Path, source_label: &str, job: &Job<'_>) -> Result<Built> {
    let final_root = destination.join(PACKAGE);
    let staging = destination.join(format!(".{PACKAGE}.build"));

    validate::check_destination(&final_root)?;

    job.report(Event::ReadingHeaders);
    let mut fragments = collect_fragments(zip)?;
    job.report(Event::HeadersRead {
        manuscripts: fragments.len(),
        groups: distinct_groups(&fragments),
    });
    sort_by_display_order(&mut fragments, |f| &f.record);
    let placed = place(&fragments)?;
    let disambiguated = placed
        .iter()
        .zip(&fragments)
        .filter(|(p, f)| p.relative != output_path(group_label(&f.record), &f.record.sigla))
        .count();

    let staging = Staging::fresh(staging)?;

    job.report(Event::WritingDocuments {
        documents: placed.len(),
    });
    // The archive this package was built from, named in the manifest so a
    // reader can tell which edition of the corpus they are looking at.
    let archive_digest = digest_of(zip)?;
    let mut applied: std::collections::BTreeMap<String, usize> = Default::default();
    let mut fonts = manifest::FontContract::default();
    let stylesheet_dropped = write_documents(
        zip,
        &fragments,
        &placed,
        staging.path(),
        &mut applied,
        &mut fonts,
        job,
    )?;

    let records: Vec<ManuscriptRecord> = fragments.iter().map(|f| f.record.clone()).collect();

    // What every document shows, decided once. Both pages are written from
    // this and neither re-derives a name, a link or a fact of its own — see
    // [`crate::presentation`].
    let corpus = crate::presentation::CorpusPresentation::linked(&records, &placed, source_label);

    let html = crate::html::render_linked_html(&corpus, "");
    let inventory = staging.path().join(format!("{PACKAGE}.html"));
    fs::write(&inventory, &html).map_err(|source| ArunaError::Io {
        path: inventory,
        source,
    })?;

    let manifest_json = manifest::render_manifest(
        &records,
        &placed,
        source_label,
        &archive_digest,
        &applied,
        &fonts,
    );
    let manifest_path = staging.path().join(MANIFEST);
    fs::write(&manifest_path, &manifest_json).map_err(|source| ArunaError::Io {
        path: manifest_path,
        source,
    })?;

    // Validation reads back everything just written; a run cancelled during
    // the write should not spend six more seconds proving it was written.
    job.check(Phase::Validating)?;
    job.report(Event::CheckingPackage);
    let staged = validate(staging.path(), &records, &placed)?;

    // Only now does it get the name. The package already there is moved aside
    // first, and put back if the publish fails.
    // The last moment at which stopping costs the reader nothing. Past this
    // line the existing package is moved aside and a new one takes its name,
    // and a run that stopped half way through that would leave the destination
    // in a state neither the old build nor the new one describes.
    job.check(Phase::Publishing)?;
    let previous = Replaced::aside(&final_root, destination)?;
    staging.publish(&final_root)?;
    for left in previous.committed() {
        job.report(Event::PreviousPackageLeft { path: &left });
    }

    job.report(Event::CheckingPublished);
    let published = validate(&final_root, &records, &placed)?;
    debug_assert_eq!(staged, published, "the rename changed the package");

    Ok(Built {
        groups: crate::parse::group_runs(&records).count(),
        documents: placed.len(),
        fragment_links: published.fragment_links,
        disambiguated,
        stylesheet_dropped,
    })
}

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
struct Replaced {
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
    committed: bool,
}

impl Replaced {
    /// Move whatever is at `target` out of the way, if anything is.
    ///
    /// `symlink_metadata`, not `exists`: `exists` follows links, and a link is
    /// what would be renamed here.
    fn aside(target: &Path, destination: &Path) -> Result<Self> {
        let mut held = Self {
            target: target.to_path_buf(),
            aside: None,
            stale: None,
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
        fs::rename(target, &aside).map_err(|source| ArunaError::Io {
            path: target.to_path_buf(),
            source,
        })?;
        held.aside = Some(aside);
        Ok(held)
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
    fn committed(mut self) -> Vec<PathBuf> {
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
            // Best effort, and the only thing left worth doing: the run has
            // already failed, and putting the reader's package back matters
            // more than reporting why the restore failed too.
            let _ = fs::rename(aside, &self.target);
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
struct Staging {
    path: PathBuf,
    published: bool,
}

impl Staging {
    /// An empty staging directory, clearing whatever an earlier run left.
    fn fresh(path: PathBuf) -> Result<Self> {
        if path.exists() {
            remove_dir(&path)?;
        }
        create_dir(&path)?;
        Ok(Self {
            path,
            published: false,
        })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Give the finished package its name. After this there is nothing to clean.
    fn publish(mut self, destination: &Path) -> Result<()> {
        fs::rename(&self.path, destination).map_err(|source| ArunaError::Io {
            path: destination.to_path_buf(),
            source,
        })?;
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

/// Pass 1: every entry the corpus's own gates accept, as a record and a path.
///
/// Headers only, exactly as the CLI reads them — the bodies are not touched
/// here. Holding all 24 000 documents to save the second pass would cost
/// several hundred megabytes for data each of which is finished with the moment
/// it is written.
pub fn collect_fragments(zip: &Path) -> Result<Vec<Fragment>> {
    let mut archive = open(zip)?;
    let mut fragments = Vec::new();
    let mut window = Vec::with_capacity(HEADER_READ_LIMIT);
    let mut path = String::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        path.clear();
        path.push_str(entry.name());
        if !is_manuscript_xml(&path) {
            continue;
        }
        window.clear();
        entry
            .by_ref()
            .take(HEADER_READ_LIMIT as u64)
            .read_to_end(&mut window)
            .map_err(|source| ArunaError::Io {
                path: PathBuf::from(&path),
                source,
            })?;
        let text = String::from_utf8_lossy(&window);
        if !looks_like_manuscript(&text) {
            continue;
        }
        fragments.push(Fragment {
            record: parse_manuscript(&path, &text),
            source: path.clone(),
        });
    }

    if fragments.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }
    Ok(fragments)
}

/// Pass 2: read each document whole, normalise it, write it where it belongs.
///
/// Returns how many carried a stylesheet instruction, which is the one thing
/// the normaliser removes that is worth counting.
fn write_documents(
    zip: &Path,
    fragments: &[Fragment],
    placed: &[Placed],
    staging: &Path,
    applied: &mut std::collections::BTreeMap<String, usize>,
    fonts: &mut manifest::FontContract,
    job: &Job<'_>,
) -> Result<usize> {
    let wanted: HashMap<&str, &Path> = fragments
        .iter()
        .zip(placed)
        .map(|(f, p)| (f.source.as_str(), p.relative.as_path()))
        .collect();

    let mut archive = open(zip)?;
    let mut written = 0usize;
    let mut dropped = 0usize;
    // Both buffers live across the loop: one document at a time, and the same
    // allocation for all 24 000 of them.
    let mut bytes = Vec::new();
    let mut normalised = Vec::new();

    for i in 0..archive.len() {
        // Between documents. Each one is inflated, normalised, checked against
        // its source and written with `create_new`; stopping here leaves the
        // staging directory holding whole documents and no half of one, and
        // the directory itself is removed on the way out because it was never
        // published. The reader's existing package is untouched throughout —
        // it is not moved aside until every document is written.
        job.check(Phase::Exporting)?;

        let mut entry = archive.by_index(i)?;
        let Some(relative) = wanted.get(entry.name()).copied() else {
            continue;
        };

        bytes.clear();
        // One byte past the limit is read on purpose: it is what tells a
        // document that fits from one that does not, and it bounds the read
        // whatever the entry claims its size to be.
        let name = entry.name().to_string();
        entry
            .by_ref()
            .take(MAX_DOCUMENT + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| ArunaError::Io {
                path: relative.to_path_buf(),
                source,
            })?;
        if bytes.len() as u64 > MAX_DOCUMENT {
            return Err(ArunaError::ExportDocumentTooLarge {
                entry: name,
                limit: MAX_DOCUMENT,
            });
        }
        if normalize::carries_stylesheet(&bytes) {
            dropped += 1;
        }
        normalised.clear();
        normalize::normalize_into(&bytes, &mut normalised);

        // Before it is written, not after. A document whose non-distortion is
        // not proven does not reach the package, and the build stops rather
        // than publishing the rest around it.
        match verify::compare(&bytes, &normalised) {
            Ok(report) => {
                // Named by `verify`, which also renders the manifest's list of
                // what is permitted. One list, so a change cannot be counted
                // under a name the manifest never advertises.
                for rule in report.dropped {
                    *applied.entry(verify::drop_pi(&rule)).or_default() += 1;
                }
                if report.added_declaration {
                    *applied
                        .entry(verify::ADD_DECLARATION.to_string())
                        .or_default() += 1;
                }
                if report.reflowed {
                    *applied
                        .entry(verify::REFLOW_PROLOGUE.to_string())
                        .or_default() += 1;
                }
            }
            Err(reason) => {
                return Err(ArunaError::ExportDistorted {
                    entry: name.clone(),
                    reason,
                })
            }
        }

        // The font contract is counted from what is actually shipped.
        fonts.observe(&String::from_utf8_lossy(&normalised));

        let out = staging.join(relative);
        if let Some(parent) = out.parent() {
            create_dir(parent)?;
        }
        // `create_new` rather than `create`: if anything ever computed the same
        // path twice, the filesystem says so instead of the second silently
        // replacing the first.
        let mut handle = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)
            .map_err(|source| ArunaError::Io {
                path: out.clone(),
                source,
            })?;
        handle
            .write_all(&normalised)
            .map_err(|source| ArunaError::Io { path: out, source })?;
        written += 1;
    }

    if written != placed.len() {
        return Err(ArunaError::ExportIncomplete {
            expected: placed.len(),
            written,
        });
    }
    Ok(dropped)
}

/// The archive's own digest, for the manifest to record.
fn digest_of(path: &Path) -> Result<String> {
    crate::md5::md5_file(path).map_err(|source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn open(zip: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(zip).map_err(|source| ArunaError::Io {
        path: zip.to_path_buf(),
        source,
    })?;
    Ok(ZipArchive::new(BufReader::with_capacity(256 * 1024, file))?)
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn remove_dir(path: &Path) -> Result<()> {
    fs::remove_dir_all(path).map_err(|source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Fragments built by hand, for the tests of every module in here.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub fn fragment(sigla: &str, cth: &str, source: &str) -> Fragment {
        Fragment {
            record: ManuscriptRecord {
                title: sigla.into(),
                sigla: sigla.into(),
                cth: Some(cth.into()),
                cth_num: cth.trim_start_matches("CTH ").parse().unwrap_or(u32::MAX),
                authorship: "AA".into(),
                year: "2020".into(),
                lang: "Hit".into(),
                inv: "—".into(),
                corpus: "HFR".into(),
            },
            source: source.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::fragment;
    use super::*;

    /// The corpus files one group under several folders, so a group's fragments
    /// are not adjacent in the archive. Counting runs of equal neighbours
    /// answered 826 groups for a corpus that has 663.
    #[test]
    fn groups_are_counted_by_how_many_there_are_not_by_how_they_are_ordered() {
        let interleaved = [
            fragment("A", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("B", "CTH 9", "root/CTH 9_XML_HFR/b.xml"),
            fragment("C", "CTH 5", "root/CTH 5_XML_TLH/c.xml"),
        ];
        assert_eq!(distinct_groups(&interleaved), 2);

        let adjacent = [
            fragment("A", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("C", "CTH 5", "root/CTH 5_XML_TLH/c.xml"),
            fragment("B", "CTH 9", "root/CTH 9_XML_HFR/b.xml"),
        ];
        assert_eq!(
            distinct_groups(&adjacent),
            distinct_groups(&interleaved),
            "the same fragments in a different order are the same groups"
        );
    }

    #[test]
    fn a_repeated_siglum_gets_a_place_of_its_own_rather_than_overwriting() {
        let fragments = vec![
            fragment("KUB 19.49+", "CTH 69", "root/CTH 69_XML_HFR/KUB 19.49+.xml"),
            fragment("KUB 19.49+", "CTH 69", "root/CTH 69_XML_TLH/KUB 19.49+.xml"),
        ];
        let placed = place(&fragments).expect("both are placed");

        assert_eq!(placed[0].relative, PathBuf::from("CTH 69/KUB 19.49+.xml"));
        assert_eq!(
            placed[1].relative,
            PathBuf::from("CTH 69/KUB 19.49+ (CTH 69_XML_TLH).xml")
        );
        assert_ne!(placed[0].relative, placed[1].relative);
        // The reader still sees the siglum, not the file name.
        assert_eq!(placed[1].label, "KUB 19.49+");
    }

    /// The same siglum in two different groups is the corpus filing one
    /// manuscript under two catalogue numbers, and both places are real.
    #[test]
    fn the_same_siglum_in_two_groups_is_not_a_collision() {
        let fragments = vec![
            fragment("KUB 26.71", "CTH 1", "root/CTH 1_XML_HFR/KUB 26.71.xml"),
            fragment("KUB 26.71", "CTH 18", "root/CTH 18_XML_HFR/KUB 26.71.xml"),
        ];
        let placed = place(&fragments).expect("two groups, two files");
        assert_eq!(placed[0].relative, PathBuf::from("CTH 1/KUB 26.71.xml"));
        assert_eq!(placed[1].relative, PathBuf::from("CTH 18/KUB 26.71.xml"));
    }

    /// Three of a kind: the suffix is taken from the archive directory, so a
    /// third document from a directory already used cannot be placed and the
    /// build says so instead of overwriting.
    #[test]
    fn a_collision_the_suffix_cannot_resolve_is_an_error_not_an_overwrite() {
        let fragments = vec![
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/a.xml"),
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/b.xml"),
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/c.xml"),
        ];
        let err = place(&fragments).expect_err("the third has nowhere to go");
        match err {
            ArunaError::ExportCollision {
                group, fragment: f, ..
            } => {
                assert_eq!(group, "CTH 5");
                assert_eq!(f, "KBo 1.1");
            }
            other => panic!("expected a collision, got {other}"),
        }
    }
}
