//! The font stack of the PDF, read from a directory the caller names.
//!
//! The seven files of `docs/FONTS.md`, checked by [`crate::fonts::verify_dir`]
//! before any byte of them is read; the system is never searched and nothing
//! is substituted (specification 3.9, requirement 1). The directory is the
//! application's resource directory in the window and an explicit path in the
//! console – there is no search here, as there is none in `verify_dir`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use typst::foundations::Bytes;
use typst::text::{Font, FontBook, FontStyle, FontWeight};
use typst::utils::LazyHash;

use super::label::Art;
use super::{Invariant, PdfError};
use crate::error::ArunaError;

/// The stack of 3.9 in its order; the main face is the regular cut. The
/// order is significant: Noto Sans Cuneiform draws cuneiform before
/// UllikummiA is asked.
pub(crate) const STACK: [&str; 5] = [
    "Noto Serif",
    "Noto Sans Cuneiform",
    "UllikummiA",
    "STIX Two Math",
    "Noto Serif Hebrew",
];

/// The face whose presence in a file calls for the credit.
pub(crate) const CREDITED: &str = "UllikummiA";

/// The fonts of one run: every face of the seven files, the book Typst
/// resolves families in, the stack's faces, and the labels made so far.
pub struct Fonts {
    pub(super) fonts: Vec<Font>,
    pub(super) book: LazyHash<FontBook>,
    /// The stack, in its order: family and index into `fonts`.
    stack: Vec<(&'static str, usize)>,
    /// The label of each code point with no glyph: made once per run, from
    /// the main face (variant B, owner's decision of 2026-09-27).
    pub(super) art: Mutex<BTreeMap<u32, Art>>,
}

impl Fonts {
    /// Reads the stack from `dir` after checking every file against the
    /// table of `docs/FONTS.md`. A missing, shortened or replaced file is the
    /// refusal [`verify_dir`](crate::fonts::verify_dir) names.
    pub fn load(dir: &Path) -> Result<Fonts, PdfError> {
        crate::fonts::verify_dir(dir)?;
        let mut fonts = Vec::new();
        for shipped in &crate::fonts::FONTS {
            let path = dir.join(shipped.file);
            let data = std::fs::read(&path).map_err(ArunaError::io(&path))?;
            fonts.extend(Font::iter(Bytes::new(data)));
        }
        Fonts::from_faces(fonts)
    }

    fn from_faces(fonts: Vec<Font>) -> Result<Fonts, PdfError> {
        let mut stack = Vec::with_capacity(STACK.len());
        for family in STACK {
            let index = fonts
                .iter()
                .position(|f| {
                    let info = f.info();
                    info.family == family
                        && info.variant.style == FontStyle::Normal
                        && info.variant.weight == FontWeight::REGULAR
                })
                .ok_or(PdfError::Invariant(Invariant::StackFace(family)))?;
            stack.push((family, index));
        }
        let book = LazyHash::new(FontBook::from_fonts(&fonts));
        Ok(Fonts {
            fonts,
            book,
            stack,
            art: Mutex::new(BTreeMap::new()),
        })
    }

    /// The face of the stack that draws a code point: the first that has it.
    /// `None` is a code point no face draws, which becomes a label.
    pub fn face(&self, cp: u32) -> Option<&'static str> {
        self.stack
            .iter()
            .find(|(_, i)| self.fonts[*i].info().coverage.contains(cp))
            .map(|(f, _)| *f)
    }

    /// The main face, regular.
    pub(super) fn main_face(&self) -> &Font {
        &self.fonts[self.stack[0].1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_without_one_of_its_faces_is_refused() {
        let Err(e) = Fonts::from_faces(Vec::new()) else {
            panic!("an empty set of faces made a stack");
        };
        assert_eq!(
            e.to_string(),
            PdfError::Invariant(Invariant::StackFace("Noto Serif")).to_string()
        );
    }
}
