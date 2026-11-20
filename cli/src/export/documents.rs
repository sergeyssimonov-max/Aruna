//! A manuscript keeps its archive identity and its output path from placement
//! through the XML and PDF passes. Only complete rows enter the package.

use super::{
    manifest, Fragment, ManuscriptRecord, PdfCount, PdfState, Placed, PlacementNames, Result,
    Tallies, Validation,
};
use std::path::Path;

pub(super) struct Document {
    pub(super) record: ManuscriptRecord,
    pub(super) place: Placed,
    pub(super) sums: manifest::Sums,
}

impl Document {
    pub(super) fn place_all(fragments: Vec<Fragment>) -> Result<Vec<Self>> {
        let mut names = PlacementNames::new(fragments.len());
        fragments
            .into_iter()
            .map(|fragment| {
                let place = names.place(&fragment)?;
                Ok(Self {
                    record: fragment.record,
                    place,
                    sums: manifest::Sums {
                        source_entry: fragment.source,
                        ..Default::default()
                    },
                })
            })
            .collect()
    }
}

/// The public renderers and validator take slices. Keep their storage private:
/// the only insertion moves one manuscript and its own PDF outcome together.
pub(super) struct PackageDocuments {
    records: Vec<ManuscriptRecord>,
    placed: Vec<Placed>,
    sums: Vec<manifest::Sums>,
    pdfs: Vec<PdfState>,
}

impl PackageDocuments {
    pub(super) fn with_capacity(n: usize) -> Self {
        Self {
            records: Vec::with_capacity(n),
            placed: Vec::with_capacity(n),
            sums: Vec::with_capacity(n),
            pdfs: Vec::with_capacity(n),
        }
    }

    pub(super) fn push(&mut self, document: Document, pdf: PdfState) {
        self.records.push(document.record);
        self.placed.push(document.place);
        self.sums.push(document.sums);
        self.pdfs.push(pdf);
    }

    pub(super) fn len(&self) -> usize {
        self.records.len()
    }

    pub(super) fn groups(&self) -> usize {
        crate::parse::group_runs(&self.records).count()
    }

    pub(super) fn pdf_count(&self) -> PdfCount {
        PdfCount::of(&self.pdfs)
    }

    pub(super) fn inventory(&self, source: &str) -> String {
        super::render_package_inventory(&self.records, &self.placed, source, &self.pdfs)
    }

    pub(super) fn manifest(&self, source: &manifest::Source<'_>, tallies: &Tallies) -> String {
        super::render_manifest(
            &self.records,
            &self.placed,
            source,
            &tallies.applied,
            &tallies.fonts,
            &tallies.xml,
            &self.sums,
            &self.pdfs,
        )
    }

    pub(super) fn validate(&self, root: &Path) -> Result<Validation> {
        super::validate(root, &self.records, &self.placed, &self.pdfs)
    }
}
