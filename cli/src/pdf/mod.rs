//! The PDF of one document (specification 4.10): pure, like [`crate::html`] –
//! a document in, the bytes of a PDF out, no file opened. Writing them is
//! `export`'s (`docs/ARCHITECTURE.md` §6).
//!
//! ```text
//! document model → layout model (layout) → Typst (world, template.typ)
//!                → PDF (typst-pdf) → labels and clusters (post)
//! ```
//!
//! What every build holds to, and checks at nearly no cost because the data
//! is already there (4.10, owner's decision of 2026-09-30, question 6):
//! Typst asks for the template and the data and nothing else; every label
//! found its image in the file; a repeated cluster is wrapped where the rule
//! finds one; the credit stands in exactly the files that embed UllikummiA.
//! A failure of any of these is a defect of this program, not of the
//! document, and stops the whole build ([`PdfError::Invariant`]). A document
//! Typst refuses is the document's failure and is recorded, not fatal
//! ([`PdfError::Document`]; `PDF-ACCEPTANCE.md` §5).
//!
//! The file carries only what does not change between runs (question 13):
//! the relative path as `ident`, "Aruna <version>" as `creator`, the template
//! version as the keywords; no date, no run identifier, nothing random.

pub mod fonts;
pub mod label;
pub mod layout;
pub mod post;
pub mod world;

use typst::foundations::Smart;
use typst_layout::PagedDocument;

use crate::document::Document;
use crate::error::ArunaError;

pub use fonts::Fonts;
pub use layout::Page;

/// The template, compiled in.
pub const TEMPLATE: &str = include_str!("template.typ");

/// The version of the template, written into every PDF as its keywords. Raised
/// with every change of `template.typ` that changes what a page shows.
pub const TEMPLATE_VERSION: &str = "aruna-pdf-template 1";

/// The width of the shift one `text:tab` makes, as `template.typ` writes it:
/// 2 em, 20 pt at the body size of 10 pt. Chosen against the page of
/// `CTH 746/KUB 28.15` set as a table in the fourth trial, where the text after
/// five tabs stands in the sixth column, about 92 pt in; five shifts of 2 em
/// put it at 100 pt (specification 4.10).
pub const TAB_EM: f32 = 2.0;

/// A program defect found while building a PDF: the build stops.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum Invariant {
    #[error("Typst asked for {0:?}, and only the template and the data exist")]
    WorldAsked(Vec<String>),
    #[error("{in_text} labels in the text, {in_file} label images in the file")]
    LabelsUnplaced { in_text: usize, in_file: usize },
    #[error("{0} label spans not found exactly once in their stream")]
    LabelSpans(usize),
    #[error("page {page}: two repeated clusters overlap at MCID {mcid}")]
    ClustersOverlap { page: u32, mcid: i64 },
    #[error("the credit was predicted {predicted} and UllikummiA embedded {embedded}")]
    Credit { predicted: bool, embedded: bool },
    #[error("the main face cannot draw a label: {0}")]
    MainFace(String),
    #[error("the font stack has no regular face of {0}")]
    StackFace(&'static str),
    #[error("the world of the template could not be built")]
    World,
    #[error("the PDF this program wrote could not be read back: {0}")]
    Pdf(String),
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    /// Typst refused this document: recorded, the build goes on.
    #[error("Typst refused the document: {}", .0.join("; "))]
    Document(Vec<String>),
    /// A defect of this program: the build stops.
    #[error("{0}")]
    Invariant(Invariant),
    /// The fonts are not the files `docs/FONTS.md` records.
    #[error(transparent)]
    Fonts(#[from] ArunaError),
}

impl PdfError {
    /// Whether this failure stops the whole build.
    pub fn is_invariant(&self) -> bool {
        !matches!(self, PdfError::Document(_))
    }
}

/// One PDF, and what building it found.
#[derive(Debug)]
pub struct Rendered {
    pub pdf: Vec<u8>,
    /// Labels set, in page order.
    pub labels: Vec<u32>,
    /// Repeated clusters wrapped in an `/ActualText`.
    pub clusters: usize,
    /// Whether the credit stands on page one.
    pub credit: bool,
    /// The paths Typst asked the world for: the template and the data.
    pub asked: std::collections::BTreeSet<String>,
}

/// What Typst said, message by message.
fn messages(errors: &[typst::diag::SourceDiagnostic]) -> Vec<String> {
    errors.iter().map(|e| e.message.to_string()).collect()
}

/// Typst's errors as the document's refusal.
fn refused(errors: typst::ecow::EcoVec<typst::diag::SourceDiagnostic>) -> PdfError {
    PdfError::Document(messages(&errors))
}

/// Builds the PDF of one document: `group` is its folder (`CTH 746`), `name`
/// its siglum, `ident` its path in the package (`CTH 746/KUB 28.15.pdf`).
pub fn render(
    doc: &Document<'_>,
    group: &str,
    name: &str,
    ident: &str,
    fonts: &Fonts,
) -> Result<Rendered, PdfError> {
    let page = Page::of(doc, group, name);
    let rendered = render_page(&page, ident, fonts, TEMPLATE);
    // The cache of Typst holds every document compiled so far: without this
    // the whole set takes 8 GB, with it 94 MiB, the PDFs the same (4.10).
    typst::comemo::evict(0);
    rendered
}

/// [`render`] of a page already laid out, with the template given.
pub fn render_page(
    page: &Page,
    ident: &str,
    fonts: &Fonts,
    template: &str,
) -> Result<Rendered, PdfError> {
    let credit = page.uses_credited_face(fonts);
    let labels = page.labels(fonts);
    let json = layout::json(fonts, page, TEMPLATE_VERSION, credit)?;
    let world =
        world::PdfWorld::new(fonts, template, json).ok_or(PdfError::Invariant(Invariant::World))?;
    let warned = typst::compile::<PagedDocument>(&world);
    let strays = world.strays();
    if !strays.is_empty() {
        return Err(PdfError::Invariant(Invariant::WorldAsked(strays)));
    }
    let mut refusal: Vec<String> = warned
        .warnings
        .iter()
        .map(|w| format!("warning: {}", w.message))
        .collect();
    let document = match warned.output {
        Ok(document) if refusal.is_empty() => document,
        Ok(_) => return Err(PdfError::Document(refusal)),
        Err(errors) => {
            refusal.extend(messages(&errors));
            return Err(PdfError::Document(refusal));
        }
    };
    let options = typst_pdf::PdfOptions {
        ident: Smart::Custom(ident.to_string()),
        creator: Smart::Custom(Some(format!("Aruna {}", env!("CARGO_PKG_VERSION")))),
        ..Default::default()
    };
    let pdf = typst_pdf::pdf(&document, &options).map_err(refused)?;
    let embedded = memchr::memmem::find(&pdf, fonts::CREDITED.as_bytes()).is_some();
    if embedded != credit {
        return Err(PdfError::Invariant(Invariant::Credit {
            predicted: credit,
            embedded,
        }));
    }
    let sequence = labels
        .iter()
        .map(|cp| {
            let art = fonts.art(*cp)?;
            Ok(post::Label {
                cp: *cp,
                height_em: art.height_em,
                depth_share: art.depth_em / art.height_em,
                width_em: art.width_em,
            })
        })
        .collect::<Result<Vec<_>, PdfError>>()?;
    let (pdf, patched) = post::finish(&pdf, &sequence, page.has_combining_marks())?;
    Ok(Rendered {
        pdf,
        labels,
        clusters: patched.clusters.len(),
        credit,
        asked: world.asked(),
    })
}

#[cfg(test)]
mod tests;
