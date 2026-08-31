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
mod lock;
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

/// The most a whole package may be.
///
/// [`MAX_DOCUMENT`] bounds one document and [`crate::archive::MAX_ENTRIES`]
/// bounds how many there are; neither bounds their sum, and the product of the
/// two is thirty terabytes. An archive of many documents each comfortably under
/// the per-document limit passes both and still fills the reader's disk — the
/// failure being a full disk in `~/Downloads`, reported as an I/O error from
/// whichever write happened to be last.
///
/// The real package is 384 MiB. Eight gibibytes is twenty times that, and a
/// corpus edition that genuinely crosses it is a decision to take deliberately
/// rather than a number to discover here.
pub const MAX_PACKAGE: u64 = 8 * 1024 * 1024 * 1024;

/// Whether the package has outgrown its ceiling, as its own function so the
/// only two sizes that matter can be tested without writing eight gibibytes.
fn within_package_ceiling(written: u64, limit: u64) -> Result<()> {
    if written > limit {
        return Err(ArunaError::ExportPackageTooLarge { written, limit });
    }
    Ok(())
}

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
/// The key two paths collide under.
///
/// **Case-folded, because the filesystem this package is written to usually is.**
/// APFS is case-insensitive by default and so is the Windows one; `KBo 1.xml`
/// and `KBo 1.XML` are one file there and two on ext4. Comparing exact paths
/// meant the export's own model said two documents and the disk held one — and
/// what stopped that from being a silent overwrite was `create_new`, three
/// hundred lines later, reporting `AlreadyExists` as an I/O error that named a
/// path and no reason.
///
/// Folding here decides it in one place instead, for every platform alike: the
/// second document is disambiguated exactly as an exact clash is, so the package
/// a Mac writes and the package a Linux machine writes are the same package.
/// The folding is Unicode `to_lowercase`, not an ASCII one. Every name in this
/// corpus is Latin sigla and digits, where the two answers agree; the wider rule
/// costs nothing and does not surprise the day a name arrives that is neither.
/// It does not touch what is written, only what is compared.
fn collision_key(relative: &Path) -> String {
    relative.to_string_lossy().to_lowercase()
}

pub fn place(fragments: &[Fragment]) -> Result<Vec<Placed>> {
    let mut taken: HashMap<String, String> = HashMap::with_capacity(fragments.len());
    let mut placed = Vec::with_capacity(fragments.len());

    for fragment in fragments {
        let group = group_label(&fragment.record);
        let base = fragment.record.sigla.as_str();

        let mut relative = output_path(group, base);
        if taken.contains_key(&collision_key(&relative)) {
            // Stable, and derived from the one thing that is unique per
            // document: where it sits in the archive.
            let suffix = disambiguator(&fragment.source);
            relative = output_path(group, &format!("{base} ({suffix})"));
        }

        if let Some(first) = taken.get(&collision_key(&relative)) {
            return Err(ArunaError::ExportCollision {
                group: group.to_string(),
                fragment: base.to_string(),
                first: first.clone(),
                second: fragment.source.clone(),
                path: relative,
            });
        }
        taken.insert(collision_key(&relative), fragment.source.clone());
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
    let staging = destination.join(staging_name());

    validate::check_destination(&final_root)?;

    job.report(Event::ReadingHeaders);
    let mut fragments = collect_fragments_with(zip, job)?;
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

    // Moved out of the fragments rather than copied out of them: the archive
    // paths are what a fragment carries beyond its record, and the last thing
    // that wanted them was the pass above. Cloning here duplicated every one of
    // 23 936 records — ten owned strings each — beside the originals, which then
    // stayed alive to the end of the build with nothing left to read them.
    let records: Vec<ManuscriptRecord> = fragments.into_iter().map(|f| f.record).collect();

    // What every document shows, decided once. Both pages are written from
    // this and neither re-derives a name, a link or a fact of its own — see
    // [`crate::presentation`].
    let corpus = crate::presentation::CorpusPresentation::linked(&records, &placed, source_label);

    let html = crate::html::render_linked_html(&corpus, "");
    let inventory = staging.path().join(crate::paths::OUTPUT_FILE_NAME);
    fs::write(&inventory, &html).map_err(ArunaError::io(inventory))?;

    let manifest_json = manifest::render_manifest(
        &records,
        &placed,
        source_label,
        &archive_digest,
        &applied,
        &fonts,
    );
    let manifest_path = staging.path().join(MANIFEST);
    fs::write(&manifest_path, &manifest_json).map_err(ArunaError::io(manifest_path))?;

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

    // **One publication at a time in this directory.** Everything up to here is
    // safe for two runs at once because each stages under a name carrying its
    // own process id. Publishing is not: it is a move, a rename and a read-back,
    // and a second run that renames in the gap leaves the first validating a
    // package it did not publish. Measured, that is exactly what happened — the
    // destination was correct every time and the losing run reported a
    // validation failure. The guard is released when it goes out of scope,
    // whichever way this function leaves.
    let _publication = lock::Publication::acquire(destination, job)?;

    // **Второй раз, и уже под замком.** Первая проверка стоит в начале сборки,
    // за шесть секунд до этой строки: между ними чужой процесс успевает
    // положить в назначение что угодно, и первая проверка о том уже ничего не
    // говорит. Здесь она стоит ровно там, где начинается необратимое, — после
    // захвата блокировки и до переименования, — так что между проверкой и
    // публикацией не может вклиниться другой прогон Aruna.
    validate::check_destination(&final_root)?;

    let previous = Replaced::aside(&final_root, destination)?;
    staging.publish(&final_root)?;
    for left in previous.committed() {
        job.report(Event::PreviousPackageLeft { path: &left });
    }

    job.report(Event::CheckingPublished);
    let published = validate(&final_root, &records, &placed)?;
    // `assert_eq!`, а не `debug_assert_eq!`: сравнение двух уже собранных
    // описей стоит микросекунды, а обещание «опубликованное равно собранному»
    // до сих пор держалось только в отладочной сборке — то есть нигде, где
    // работает читатель.
    assert_eq!(staged, published, "the rename changed the package");

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
        fs::rename(target, &aside).map_err(ArunaError::io(&target))?;
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
/// **What this gives up.** A run killed with a signal leaves its staging
/// directory behind, and the next run no longer clears it by finding it under
/// the name it would have used itself — it simply builds beside it. That
/// leftover is a real cost, up to the size of a package, and it is the price
/// of two runs not destroying each other. Everything else about recovery is
/// unchanged: a build that *fails* still clears its own staging through
/// `Drop`, and an orphaned published copy is still swept by [`Replaced`].
fn staging_name() -> String {
    format!(".{PACKAGE}.build.{}", crate::paths::run_tag())
}

/// Pass 1: every entry the corpus's own gates accept, as a record and a path.
///
/// Headers only, exactly as the CLI reads them — the bodies are not touched
/// here. Holding all 24 000 documents to save the second pass would cost
/// several hundred megabytes for data each of which is finished with the moment
/// it is written.
pub fn collect_fragments(zip: &Path) -> Result<Vec<Fragment>> {
    collect_fragments_with(zip, &Job::unattended())
}

/// The same scan, told what may stop it.
///
/// **A quarter of the run used to be unstoppable.** The scan reads a header
/// window out of every entry in the archive, and on the real corpus that is
/// 1,6 seconds of the 6,3 the build takes — measured on 2026-08-30. Nothing in
/// it asked whether the run had been cancelled, so a flag raised at
/// `ReadingHeaders` was not seen until the write loop's first check, and the
/// window's Cancel button would have been dead for that quarter.
///
/// The cost of noticing is one relaxed atomic load per entry, at a boundary
/// where stopping leaves nothing behind: no file is open and nothing has been
/// written yet.
///
/// Kept as a second function rather than a changed signature, because
/// `collect_fragments` is public and a caller that has no job should not have
/// to invent one.
pub fn collect_fragments_with(zip: &Path, job: &Job<'_>) -> Result<Vec<Fragment>> {
    let mut archive = open(zip)?;
    let mut fragments = Vec::new();
    let mut window = Vec::with_capacity(HEADER_READ_LIMIT);
    let mut path = String::new();

    for i in 0..archive.len() {
        job.check(Phase::Exporting)?;
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
            .map_err(ArunaError::io(&path))?;
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
    // **One slot per entry name, and the archive is held to it here.**
    //
    // `collect` on a map keeps the last value for a repeated key, so an archive
    // with two entries of the same name used to lose one of the two places
    // `place` had reserved. Both entries then resolved to the surviving path:
    // the first write took it, the second met `create_new` and failed as
    // `AlreadyExists` on a path — an I/O error that named the destination and
    // said nothing about the archive that caused it, three hundred lines away.
    // Inserting one at a time turns that into a sentence about the archive.
    let mut wanted: HashMap<&str, &Path> = HashMap::with_capacity(fragments.len());
    for (fragment, placement) in fragments.iter().zip(placed) {
        if wanted
            .insert(fragment.source.as_str(), placement.relative.as_path())
            .is_some()
        {
            return Err(ArunaError::ArchiveDuplicateEntry {
                entry: fragment.source.clone(),
            });
        }
    }

    let mut archive = open(zip)?;
    let mut written = 0usize;
    let mut dropped = 0usize;
    // The package's own size, accumulated as it is written rather than measured
    // afterwards: the point of the ceiling is to stop before the disk is full,
    // and a check after the last write is a check after the damage.
    let mut package_bytes = 0u64;
    // Both buffers live across the loop: one document at a time, and the same
    // allocation for all 24 000 of them.
    let mut bytes = Vec::new();
    let mut normalised = Vec::new();
    // See the write below: the directory this loop made last.
    let mut last_dir: Option<PathBuf> = None;

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
            .map_err(ArunaError::io(relative))?;
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
            // The last directory made, remembered. The archive lists a group's
            // documents together, so 23 936 documents ask about 663
            // directories — and `create_dir_all` on one that exists is still a
            // syscall per document, measured at 219 ms of a six-second run.
            //
            // Only ever skips a directory this loop made itself, moments ago,
            // inside a staging directory named for this process. A cache that
            // is wrong is a cache that is stale, and there is nothing here to
            // go stale against.
            if last_dir.as_deref() != Some(parent) {
                create_dir(parent)?;
                last_dir = Some(parent.to_path_buf());
            }
        }
        // `create_new` rather than `create`: if anything ever computed the same
        // path twice, the filesystem says so instead of the second silently
        // replacing the first.
        let mut handle = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)
            .map_err(ArunaError::io(&out))?;
        handle.write_all(&normalised).map_err(ArunaError::io(out))?;
        written += 1;
        package_bytes = package_bytes.saturating_add(normalised.len() as u64);
        within_package_ceiling(package_bytes, MAX_PACKAGE)?;
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
    crate::md5::md5_file(path).map_err(ArunaError::io(path))
}

/// The same gate the inventory pass opens through.
///
/// Both passes read the same archive, and an entry count this program refuses
/// in one of them is not one it should accept in the other — the export used to
/// open the file itself, so `MAX_ENTRIES` applied to the first pass and not to
/// the second.
fn open(zip: &Path) -> Result<ZipArchive<BufReader<File>>> {
    crate::archive::open_zip(zip)
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(ArunaError::io(path))
}

fn remove_dir(path: &Path) -> Result<()> {
    fs::remove_dir_all(path).map_err(ArunaError::io(path))
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

    /// **An archive that names one entry twice is stopped by name.**
    ///
    /// The map from an entry's name to its place in the package has one slot
    /// per name, and `collect` used to fill it by keeping the last writer: one
    /// of the two reserved places disappeared, both entries then resolved to
    /// the survivor, and the second write failed as `AlreadyExists` on a path —
    /// an I/O error that named the destination and not the archive.
    ///
    /// The archive path here does not exist, and that is the second half of
    /// what this checks: the failure must come before anything is opened, so
    /// the duplicate is reported as a fact about the input rather than as
    /// whatever the reader happens to hit first.
    #[test]
    fn an_archive_that_names_one_entry_twice_is_refused_by_name() {
        let fragments = vec![
            fragment("KBo 1.1", "CTH 5", "xml/KBo 1.1.xml"),
            fragment("KBo 2.2", "CTH 5", "xml/KBo 1.1.xml"),
        ];
        let placed = place(&fragments).expect("two distinct sigla take two places");
        let mut applied = std::collections::BTreeMap::new();
        let mut fonts = manifest::FontContract::default();

        let failure = write_documents(
            Path::new("/nowhere/there-is-no-archive.zip"),
            &fragments,
            &placed,
            Path::new("/nowhere/staging"),
            &mut applied,
            &mut fonts,
            &Job::unattended(),
        )
        .expect_err("a duplicated entry name must not be written");

        match failure {
            ArunaError::ArchiveDuplicateEntry { entry } => {
                assert_eq!(entry, "xml/KBo 1.1.xml");
            }
            other => panic!("expected the duplicate to be named, got {other}"),
        }
    }

    /// Two documents whose names differ only in case are two documents, and on
    /// most filesystems one file.
    ///
    /// Placement used to compare exact paths, so its model said two while APFS
    /// and NTFS held one; the write then failed with `AlreadyExists` from
    /// `create_new`, three hundred lines away from the decision that caused it.
    /// Both are disambiguated now, on every platform, so the package does not
    /// depend on the filesystem it was built on.
    #[test]
    fn two_names_that_differ_only_in_case_are_placed_as_two_files() {
        let fragments = [
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_HFR/KBo 1.1.xml"),
            fragment("KBo 1.1", "CTH 5", "root/CTH 5_XML_TLH/KBo 1.1.xml"),
            fragment("kbo 1.1", "CTH 5", "root/CTH 5_XML_ANO/kbo 1.1.xml"),
        ];
        let placed = place(&fragments).expect("all three are placed");

        let keys: std::collections::HashSet<String> =
            placed.iter().map(|p| collision_key(&p.relative)).collect();
        assert_eq!(
            keys.len(),
            3,
            "three documents must occupy three paths even where case does not distinguish them: {:?}",
            placed.iter().map(|p| p.relative.clone()).collect::<Vec<_>>()
        );
    }

    /// A ceiling on the package as a whole, at the only two sizes where it can
    /// be wrong. Eight gibibytes are not written to test eight gibibytes.
    #[test]
    fn the_package_ceiling_admits_its_own_limit_and_refuses_one_byte_more() {
        assert!(within_package_ceiling(1024, 1024).is_ok());
        match within_package_ceiling(1025, 1024) {
            Err(ArunaError::ExportPackageTooLarge { written, limit }) => {
                assert_eq!((written, limit), (1025, 1024));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

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
