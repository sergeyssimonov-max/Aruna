//! The PDF phase of the export: one PDF beside each XML document of the
//! package (owner's decisions of 2026-09-30, questions 1, 2, 3 and 11).
//!
//! It runs after the documents are written and before the inventory and the
//! manifest, inside the staging directory: a cancelled or failed run leaves
//! the reader's folder as it was by the same mechanism as every other phase.
//! Each document is read back from the staging directory, not held in memory
//! from its writing, so memory stays bounded; the cache of Typst is emptied
//! after each one ([`crate::pdf::render`]).
//!
//! A document Typst refuses, or one the model refuses, gets no PDF and a
//! record in the manifest, and the build goes on (`PDF-ACCEPTANCE.md` §5) –
//! as does one with two repeated clusters in a row (owner's decision of
//! 2026-10-02). A broken invariant of this program stops the whole build and
//! names the document and the invariant.

use std::fs;
use std::path::{Path, PathBuf};

use crate::document::Document;
use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use crate::parse::{group_label, ManuscriptRecord};
use crate::pdf::{Fonts, PdfError};
use crate::progress::Event;

use super::naming::{dir_component, pdf_path};
use super::Placed;

/// Whether the package carries a PDF of every document.
#[derive(Clone, Copy)]
pub enum Pdf<'f> {
    /// The package as before: XML documents, the inventory, the manifest.
    Off,
    /// And a PDF beside each document, set in these fonts.
    On(&'f Fonts),
}

/// What became of one document's PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfState {
    /// Written, at this path relative to the package root.
    Built(PathBuf),
    /// Not written, and why: the model refused the document, or Typst did.
    Refused(String),
}

impl PdfState {
    pub fn built(&self) -> Option<&Path> {
        match self {
            PdfState::Built(p) => Some(p),
            PdfState::Refused(_) => None,
        }
    }
}

/// How often the phase says how far it is: the PDFs take about a hundred
/// seconds on the whole corpus, a tick every 250 is two or three a second.
const PDFS_PER_TICK: usize = 250;

/// Builds the PDF of every placed document, in the order they were placed –
/// the one display order of the program – and writes each beside its XML.
pub(crate) fn write_pdfs(
    staging: &Path,
    records: &[ManuscriptRecord],
    placed: &[Placed],
    fonts: &Fonts,
    package_bytes: &mut u64,
    job: &Job<'_>,
) -> Result<Vec<PdfState>> {
    job.report(Event::WritingPdfs {
        documents: placed.len(),
    });
    let mut states = Vec::with_capacity(placed.len());
    let mut built = 0usize;
    for (n, (record, place)) in records.iter().zip(placed).enumerate() {
        // Between documents, as the XML phase asks: stopping here leaves the
        // staging directory to remove itself.
        job.check(Phase::Exporting)?;
        let state = one(staging, record, place, fonts)?;
        if let PdfState::Built(relative) = &state {
            built += 1;
            let bytes = fs::metadata(staging.join(relative))
                .map_err(ArunaError::io(staging.join(relative)))?
                .len();
            *package_bytes = package_bytes.saturating_add(bytes);
            super::within_package_ceiling(*package_bytes, super::MAX_PACKAGE)?;
        }
        states.push(state);
        let done = n + 1;
        if done.is_multiple_of(PDFS_PER_TICK) || done == placed.len() {
            job.report(Event::PdfsWritten {
                done,
                built,
                total: placed.len(),
            });
        }
    }
    Ok(states)
}

/// A broken invariant as the export tells a package that does not match its
/// model: not published, a defect of the program and not of the data – the
/// sentence the window already has for it. The document and the invariant are
/// the first problem, two parts the console translates one by one.
pub(crate) fn invariant_broken(root: &Path, document: &str, invariant: &str) -> ArunaError {
    ArunaError::ExportInvalid {
        root: root.to_path_buf(),
        count: 1,
        first: format!("the PDF of {document} broke an invariant of this program; {invariant}"),
    }
}

/// One document: read back, laid out, built, written.
fn one(
    staging: &Path,
    record: &ManuscriptRecord,
    place: &Placed,
    fonts: &Fonts,
) -> Result<PdfState> {
    let xml_path = staging.join(&place.relative);
    let bytes = fs::read(&xml_path).map_err(ArunaError::io(&xml_path))?;
    let doc = match Document::read(&bytes) {
        Ok(doc) => doc,
        Err(refusal) => return Ok(PdfState::Refused(format!("the document model: {refusal}"))),
    };
    let relative = pdf_path(&place.relative);
    let ident = relative.to_string_lossy();
    let group = dir_component(group_label(record));
    match crate::pdf::render(&doc, &group, &place.label, &ident, fonts) {
        Ok(rendered) => {
            let out = staging.join(&relative);
            // `create_new`, as the XML phase writes: two documents never share
            // a PDF, because two never share an XML path (`place`).
            let mut handle = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&out)
                .map_err(ArunaError::io(&out))?;
            std::io::Write::write_all(&mut handle, &rendered.pdf).map_err(ArunaError::io(&out))?;
            Ok(PdfState::Built(relative))
        }
        Err(PdfError::Document(messages)) => {
            Ok(PdfState::Refused(format!("Typst: {}", messages.join("; "))))
        }
        // Owner's decision of 2026-10-02: this document only, recorded.
        Err(clusters @ PdfError::Clusters { .. }) => Ok(PdfState::Refused(clusters.to_string())),
        Err(PdfError::Fonts(error)) => Err(error),
        Err(PdfError::Invariant(invariant)) => Err(invariant_broken(
            staging,
            &place.relative.to_string_lossy(),
            &invariant.to_string(),
        )),
    }
}
