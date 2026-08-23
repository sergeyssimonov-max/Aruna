//! What the program does, named once, for every front end that asks.
//!
//! Below this line are modules that each know one thing: how to read an
//! archive, where a file goes, what a document looks like. Above it are callers
//! that know none of that and should not learn — a `main` with four lines in
//! it, and later a Tauri command with two.
//!
//! In between there was nothing. `main.rs` called `aruna::run`, and a window
//! would have called `export::build` and then assembled the outcome itself: how
//! many groups, how many documents, where the package went, what to say if it
//! stopped. Two callers doing that separately is two answers to the same
//! question, and the second one is written months later by someone reading the
//! first.
//!
//! So the scenarios are here, each one a function that takes a request and a
//! [`Job`] and returns a report:
//!
//! ```text
//!   CLI ──┐     app::build_corpus(…)     ──►  both of the below, one archive
//!         ├──►  app::build_inventory(…)  ──►  archive, html, paths
//!   Tauri ┘     app::build_package(…)    ──►  export, presentation, manifest
//!         ▲              │
//!         └── Report ────┘   typed, owned, and free of anything a window
//!                            cannot be handed
//! ```
//!
//! **Owned, not borrowed.** Everything below this module borrows, because it
//! runs in one pass over data the caller holds. A report is the opposite: it
//! outlives the call, and a window's copy has to survive the archive being
//! dropped. This is the layer where that changes, and it changes once.
//!
//! **No Tauri, no serde, no window.** A report is a plain Rust struct. Making
//! it serialisable is a derive on this module's types and a dependency in the
//! manifest, added when there is something to serialise it for; putting it here
//! now would be a dependency with no consumer, and the derive would want to
//! spread inward the moment anything nested needed it. `docs/FRONTEND-CONTRACT.md`
//! §2.1 is the standing decision that it must not.

use crate::error::{ArunaError, Result};
use crate::job::{Job, JobId, Phase};
use crate::progress::Event;
use std::path::{Path, PathBuf};

/// Build the standalone inventory the CLI writes to `~/Downloads`.
///
/// The whole of what `main` used to do, minus the printing.
#[derive(Debug, Clone, Default)]
pub struct InventoryRequest {
    /// An archive to read instead of downloading one.
    ///
    /// `None` means the pinned Zenodo record, through the cache. Tests and
    /// offline runs pass a path; so will a window that lets someone choose a
    /// file they already have.
    pub local_archive: Option<PathBuf>,
}

/// What building the inventory came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReport {
    pub job: JobId,
    /// The document that was written.
    pub output: PathBuf,
    pub manuscripts: usize,
    pub groups: usize,
}

/// Build the package: a folder of documents with an inventory over them.
#[derive(Debug, Clone)]
pub struct PackageRequest {
    /// The archive to build from. Required — unlike the inventory, there is no
    /// default worth guessing at.
    pub archive: PathBuf,
    /// Where the package goes. The exporter creates and owns a directory
    /// inside it and refuses one it did not create.
    pub destination: PathBuf,
    /// The attribution line every document carries.
    pub source_label: String,
}

/// What building the package came to.
///
/// The counts are the ones the build returned rather than a second count taken
/// afterwards: a report that recounted could disagree with the manifest, and
/// this project has had exactly that defect before — a progress line that said
/// 826 groups beside a summary that said 663.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReport {
    pub job: JobId,
    /// The package's root directory.
    pub root: PathBuf,
    pub documents: usize,
    pub groups: usize,
    /// Documents that needed a suffix because their siglum was taken.
    pub disambiguated: usize,
    /// Documents that carried a stylesheet instruction the package does
    /// without.
    pub stylesheet_dropped: usize,
}

/// Read the corpus and write the standalone inventory.
///
/// Reports `Indexed` on the way through, so a window can show what was found
/// before the document is written.
pub fn build_inventory(request: &InventoryRequest, job: &Job<'_>) -> Result<InventoryReport> {
    let output = crate::run(request.local_archive.as_deref(), job)?;

    // Counted from the document just written rather than by parsing the archive
    // a second time. `run` does not hand these back — it returns a path,
    // because that is what its caller has always needed — and re-reading 71 MiB
    // to learn a number the run already knew would be the most expensive line
    // in this module.
    let (manuscripts, groups) = counts_from_inventory(&output)?;

    Ok(InventoryReport {
        job: job.id(),
        output,
        manuscripts,
        groups,
    })
}

/// Build the package, and say what is in it.
pub fn build_package(request: &PackageRequest, job: &Job<'_>) -> Result<PackageReport> {
    let built = crate::export::build(
        &request.archive,
        &request.destination,
        &request.source_label,
        job,
    )?;

    Ok(PackageReport {
        job: job.id(),
        root: request.destination.join(crate::export::PACKAGE),
        documents: built.documents,
        groups: built.groups,
        disambiguated: built.disambiguated,
        stylesheet_dropped: built.stylesheet_dropped,
    })
}

/// Build both artifacts from one archive: the inventory, and the package.
///
/// What the binary does. The two scenarios above are the halves — this is the
/// whole, and it exists because the halves were never wired together: the
/// program wrote the inventory and nothing else, while the export that turns
/// the corpus into folders of documents was reachable only from an example.
/// A reader who opened the inventory found a table with nothing to click.
#[derive(Debug, Clone, Default)]
pub struct CorpusRequest {
    /// An archive to read instead of downloading one. As [`InventoryRequest`].
    pub local_archive: Option<PathBuf>,
}

/// What building the corpus came to: one report for each artifact.
///
/// Kept as the two reports rather than flattened into one struct with six
/// fields. They are answers to different questions — how many manuscripts the
/// inventory lists, and what the package on disk contains — and a caller that
/// wants only one should not have to know which fields belong to which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusReport {
    pub job: JobId,
    pub inventory: InventoryReport,
    pub package: PackageReport,
}

/// Both artifacts, from one archive, in one run.
///
/// **The archive is resolved once.** [`crate::run`] would fetch or open it
/// again, and the export a third time; resolving here and handing the same
/// path to both means one download, one cache lookup, one digest check. The
/// two passes that remain are over the bytes on disk, and they read different
/// things out of them — records for the inventory, fragments and placements
/// for the package — which is why they are two passes and not one.
///
/// **The inventory is written first, and that is the order cancellation
/// needs.** Stopping in the middle of the package leaves the inventory
/// standing and no half-package behind it: the export builds under a staging
/// name and takes the final one last. Stopping the other way round would leave
/// a package whose inventory was never written.
pub fn build_corpus(request: &CorpusRequest, job: &Job<'_>) -> Result<CorpusReport> {
    let source = match &request.local_archive {
        Some(path) => crate::cache::Archive::Cached(path.clone()),
        None => crate::obtain_archive(
            crate::download::ZENODO_ZIP_URL,
            crate::download::ZENODO_ZIP_MD5,
            job,
        )?,
    };

    // Both phases are handed the resolved path, so neither downloads.
    let archive = source.path().to_path_buf();

    let inventory = build_inventory(
        &InventoryRequest {
            local_archive: Some(archive.clone()),
        },
        job,
    )?;

    // Beside the inventory, from the same helper that decided where the
    // inventory goes — so an overridden `HOME` moves both together.
    let destination = inventory
        .output
        .parent()
        .ok_or_else(|| ArunaError::DownloadsDir)?
        .to_path_buf();

    let package = build_package(
        &PackageRequest {
            archive: archive.clone(),
            destination,
            source_label: crate::SOURCE_LABEL.to_string(),
        },
        job,
    )?;

    // This run's own copy, if that is what it was: the inventory phase was
    // handed it as `Cached` and so left it alone.
    if let crate::cache::Archive::Temporary(path) = &source {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    Ok(CorpusReport {
        job: job.id(),
        inventory,
        package,
    })
}

/// The two counts the inventory prints, read back out of it.
///
/// The document states them in one line, in a shape `html::write_summary`
/// writes and nothing else produces. Reading them back is cheap — the file is
/// under a megabyte — and it keeps this module from being a second opinion
/// about what the corpus contains.
fn counts_from_inventory(path: &Path) -> Result<(usize, usize)> {
    let html = std::fs::read_to_string(path).map_err(|source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let after = |needle: &str| -> usize {
        html.split_once(needle)
            .and_then(|(_, rest)| {
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .unwrap_or(0)
    };
    Ok((after("Manuscripts: "), after("Groups (CTH): ")))
}

// ---------------------------------------------------------------------------
// The error, as something other than a Rust type
// ---------------------------------------------------------------------------

/// A failure in the shape a front end can act on.
///
/// [`ArunaError`] is right for Rust: it carries a source, a path, a status, and
/// `main` matches on it exhaustively to decide what to advise. None of that
/// crosses a process boundary well. A window needs to know *what kind* of thing
/// went wrong without parsing English, whether trying again could help, and
/// where in the run it happened — and it must not be handed a path from the
/// machine's filesystem or the inside of a backtrace.
///
/// So this is the translation, made once. It is deliberately small: a stable
/// code, a phase, a sentence, and a flag. Anything a front end cannot use is
/// not in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// A stable, machine-readable kind. Never a Rust type name — those change
    /// under a rename and this is a contract.
    pub code: &'static str,
    /// Where in the run it happened, when that is known.
    pub phase: Option<Phase>,
    /// One sentence, for a person.
    pub message: String,
    /// Whether running the same thing again could plausibly succeed.
    ///
    /// A network timeout, yes. A checksum mismatch, no — the digest will not
    /// change on the second try, and an interface that offers *Retry* for it is
    /// lying.
    pub retryable: bool,
    /// True for the one outcome that is not a fault: the caller stopped it.
    pub cancelled: bool,
}

impl Failure {
    /// Translate an error into what a front end is told.
    pub fn of(error: &ArunaError) -> Failure {
        use ArunaError::*;
        let (code, phase, retryable) = match error {
            Cancelled { phase } => ("cancelled", Some(*phase), false),

            // The network, and what a second attempt can fix.
            Network { .. } => ("network", Some(Phase::Obtaining), true),
            // The statuses that mean "not right now" rather than "not this
            // request" — the same set `download::is_retryable_status` acts on,
            // stated here as a pattern because a front end offering *Retry* has
            // to agree with the client that does the retrying.
            Http {
                status: 408 | 425 | 429 | 500..=599,
                ..
            } => ("server_busy", Some(Phase::Obtaining), true),
            Http { .. } => ("http", Some(Phase::Obtaining), false),
            Truncated { .. } => ("truncated", Some(Phase::Obtaining), true),
            Oversized { .. } => ("oversized", Some(Phase::Obtaining), false),
            // The archive was republished; retrying downloads the same bytes
            // and gets the same digest.
            ChecksumMismatch { .. } => ("checksum", Some(Phase::Obtaining), false),

            // The corpus.
            Zip(_) => ("archive_unreadable", Some(Phase::Parsing), false),
            EmptyArchive => ("archive_empty", Some(Phase::Parsing), false),

            // Where the output goes.
            DownloadsDir => ("no_output_directory", Some(Phase::Publishing), false),
            Replace { .. } => ("output_locked", Some(Phase::Publishing), true),
            Io { .. } => ("io", None, true),

            // The export refusing to do something wrong.
            ExportCollision { .. } => ("collision", Some(Phase::Exporting), false),
            ExportDocumentTooLarge { .. } => ("document_too_large", Some(Phase::Exporting), false),
            ExportDistorted { .. } => ("distorted", Some(Phase::Exporting), false),
            ExportDestination { .. } => ("destination_not_ours", Some(Phase::Exporting), false),
            ExportInvalid { .. } => ("package_invalid", Some(Phase::Validating), false),
            ExportIncomplete { .. } => ("package_incomplete", Some(Phase::Validating), false),
        };

        Failure {
            code,
            phase,
            message: error.to_string(),
            retryable,
            cancelled: matches!(error, Cancelled { .. }),
        }
    }
}

impl From<&ArunaError> for Failure {
    fn from(error: &ArunaError) -> Failure {
        Failure::of(error)
    }
}

/// Say a stage has started, without the caller building the event itself.
///
/// A convenience with a purpose: it is the one place a scenario reports, so a
/// scenario that wanted to invent its own vocabulary would have to do it
/// visibly.
pub fn announce(job: &Job<'_>, event: Event<'_>) {
    job.report(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::Cancel;
    use crate::progress::Silent;
    use tempfile::tempdir;

    fn archive(dir: &Path, n: usize) -> PathBuf {
        use std::io::Write as _;
        let path = dir.join("corpus.zip");
        let file = std::fs::File::create(&path).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for i in 0..n {
            zip.start_file(format!("root/CTH {}_XML_HFR/d{i}.xml", i % 3), options)
                .expect("entry");
            let body = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><AOxml xml:space="preserve"><AOHeader><docID>KBo {i}</docID><meta><uebern editor="FB" date="2017-03-28"/></meta></AOHeader><body><text><l lg="Hit"/>t</text></body></AOxml>"#
            );
            zip.write_all(body.as_bytes()).expect("write");
        }
        zip.finish().expect("finish");
        path
    }

    /// The package scenario reports what was built, from the build itself.
    #[test]
    fn building_a_package_reports_what_is_in_it() {
        let dir = tempdir().expect("tempdir");
        let zip = archive(dir.path(), 9);
        let destination = dir.path().join("out");
        std::fs::create_dir(&destination).expect("destination");

        let job = Job::unattended();
        let report = build_package(
            &PackageRequest {
                archive: zip,
                destination: destination.clone(),
                source_label: "test".into(),
            },
            &job,
        )
        .expect("the package builds");

        assert_eq!(
            report.job,
            job.id(),
            "the report names the run it describes"
        );
        assert_eq!(report.documents, 9);
        assert_eq!(report.groups, 3);
        assert_eq!(report.root, destination.join(crate::export::PACKAGE));
        assert!(
            report.root.is_dir(),
            "the report names a package that is there"
        );
    }

    /// A cancelled scenario reports the cancellation rather than a partial
    /// success — there is no half-built package to describe.
    #[test]
    fn a_cancelled_scenario_returns_the_cancellation() {
        let dir = tempdir().expect("tempdir");
        let zip = archive(dir.path(), 9);
        let destination = dir.path().join("out");
        std::fs::create_dir(&destination).expect("destination");

        let cancel = Cancel::new();
        cancel.cancel();
        let outcome = build_package(
            &PackageRequest {
                archive: zip,
                destination,
                source_label: "test".into(),
            },
            &Job::new(&Silent, &cancel),
        );

        let error = outcome.expect_err("a cancelled run does not report success");
        let failure = Failure::of(&error);
        assert_eq!(failure.code, "cancelled");
        assert!(failure.cancelled);
        assert!(!failure.retryable, "there is nothing to retry");
    }

    /// Every error has a code, no two kinds share one by accident, and none of
    /// them is a Rust type name.
    #[test]
    fn every_failure_has_a_stable_code() {
        let cases = [
            ArunaError::Cancelled {
                phase: Phase::Parsing,
            },
            ArunaError::EmptyArchive,
            ArunaError::DownloadsDir,
            ArunaError::Http {
                url: "u".into(),
                status: 503,
                retry_after: None,
            },
            ArunaError::Http {
                url: "u".into(),
                status: 404,
                retry_after: None,
            },
            ArunaError::Truncated {
                url: "u".into(),
                expected: 10,
                got: 4,
            },
            ArunaError::ChecksumMismatch {
                url: "u".into(),
                expected: "a".into(),
                got: "b".into(),
            },
        ];
        for error in &cases {
            let failure = Failure::of(error);
            assert!(!failure.code.is_empty());
            assert!(
                failure
                    .code
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{} is not a stable machine code",
                failure.code
            );
            assert!(
                !failure.message.is_empty(),
                "{} has no sentence",
                failure.code
            );
        }
    }

    /// A busy server is worth another try; a wrong digest is not.
    ///
    /// The distinction the flag exists for. An interface that offered *Retry*
    /// for a checksum mismatch would be inviting someone to download 71 MiB
    /// again to get the same answer.
    #[test]
    fn only_the_failures_a_second_attempt_could_fix_are_retryable() {
        let retryable = [
            ArunaError::Http {
                url: "u".into(),
                status: 503,
                retry_after: Some(30),
            },
            ArunaError::Truncated {
                url: "u".into(),
                expected: 10,
                got: 4,
            },
        ];
        for error in &retryable {
            assert!(Failure::of(error).retryable, "{error} should be retryable");
        }

        let settled = [
            ArunaError::ChecksumMismatch {
                url: "u".into(),
                expected: "a".into(),
                got: "b".into(),
            },
            ArunaError::EmptyArchive,
            ArunaError::Http {
                url: "u".into(),
                status: 404,
                retry_after: None,
            },
            ArunaError::Cancelled {
                phase: Phase::Exporting,
            },
        ];
        for error in &settled {
            assert!(
                !Failure::of(error).retryable,
                "{error} is offered as retryable and cannot be fixed by retrying"
            );
        }
    }

    /// The phase travels with the failure, so a window can say where.
    #[test]
    fn a_failure_carries_the_stage_it_happened_in() {
        assert_eq!(
            Failure::of(&ArunaError::EmptyArchive).phase,
            Some(Phase::Parsing)
        );
        assert_eq!(
            Failure::of(&ArunaError::Cancelled {
                phase: Phase::Publishing
            })
            .phase,
            Some(Phase::Publishing)
        );
        // An I/O failure can happen in any of them, and claiming one would be
        // worse than saying nothing.
        assert_eq!(
            Failure::of(&ArunaError::Io {
                path: "/x".into(),
                source: std::io::Error::other("busy")
            })
            .phase,
            None
        );
    }
}
