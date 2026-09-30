//! The letters of a label, `U+XXXX`, as the outlines of the main face: an SVG
//! the template sets as an image, so the letters are paths and never text
//! (variant B, owner's decisions of 2026-09-27 and 2026-09-28 – the one thing
//! drawn as outlines is the name of a missing sign, never text of a document).

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::MetadataProvider;
use std::sync::PoisonError;

use super::fonts::Fonts;
use super::{Invariant, PdfError};

/// The size of the label's letters against the text around it.
pub const LABEL_EM: f32 = 0.62;

/// `luma(35%)`, the grey of the label's frame, for its letters.
const LABEL_FILL: &str = "#595959";

/// One label, drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    pub svg: String,
    /// Height of the image and its depth below the baseline, and its width,
    /// in em of the surrounding text.
    pub height_em: f32,
    pub depth_em: f32,
    pub width_em: f32,
}

/// Writes a glyph outline as SVG path data, y turned down, `dx` along.
struct SvgPath<'a> {
    d: &'a mut String,
    dx: f32,
    /// The lowest and highest point drawn: the extent of the letters.
    low: f32,
    high: f32,
}

impl SvgPath<'_> {
    fn pt(&mut self, x: f32, y: f32) {
        self.d.push_str(&format!(" {:.1} {:.1}", x + self.dx, -y));
        self.low = self.low.min(y);
        self.high = self.high.max(y);
    }
}

impl OutlinePen for SvgPath<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.d.push('M');
        self.pt(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.d.push('L');
        self.pt(x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.d.push('Q');
        self.pt(x1, y1);
        self.pt(x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.d.push('C');
        self.pt(x1, y1);
        self.pt(x2, y2);
        self.pt(x, y);
    }
    fn close(&mut self) {
        self.d.push('Z');
    }
}

impl Fonts {
    /// The label of a code point: made once, from the regular cut of the main
    /// face, then served from the cache of the run.
    pub fn art(&self, cp: u32) -> Result<Art, PdfError> {
        // A poisoned cache holds labels drawn before the panic that poisoned
        // it, each complete: a label is inserted whole or not at all.
        let mut cache = self.art.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(art) = cache.get(&cp) {
            return Ok(art.clone());
        }
        let art = draw(self, cp)?;
        cache.insert(cp, art.clone());
        Ok(art)
    }
}

/// The main face failed to give the letters of a label: the face of the tree
/// draws `U`, `+` and the hex digits, so this is a defect, not the document's.
fn unreadable(e: impl std::fmt::Display) -> PdfError {
    PdfError::Invariant(Invariant::MainFace(e.to_string()))
}

fn draw(fonts: &Fonts, cp: u32) -> Result<Art, PdfError> {
    let font = fonts.main_face();
    let face = skrifa::FontRef::from_index(font.data(), font.index()).map_err(unreadable)?;
    let upem = f32::from(
        face.metrics(Size::unscaled(), LocationRef::default())
            .units_per_em,
    );
    let outlines = face.outline_glyphs();
    let metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
    let (mut d, mut x, mut low, mut high) = (String::new(), 0.0f32, 0.0f32, 0.0f32);
    for c in format!("U+{cp:04X}").chars() {
        let glyph = face
            .charmap()
            .map(c)
            .ok_or(Invariant::MainFace(format!(
                "no glyph for {c:?} of a label"
            )))
            .map_err(PdfError::Invariant)?;
        let mut path = SvgPath {
            d: &mut d,
            dx: x,
            low: 0.0,
            high: 0.0,
        };
        if let Some(outline) = outlines.get(glyph) {
            outline
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                    &mut path,
                )
                .map_err(unreadable)?;
        }
        low = low.min(path.low);
        high = high.max(path.high);
        x += metrics.advance_width(glyph).unwrap_or(0.0);
    }
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{x:.0}\" height=\"{:.0}\" viewBox=\"0 {:.0} {x:.0} {:.0}\"><path fill=\"{LABEL_FILL}\" d=\"{}\"/></svg>",
        high - low,
        -high,
        high - low,
        d.trim_start()
    );
    Ok(Art {
        svg,
        height_em: (high - low) / upem * LABEL_EM,
        width_em: x / upem * LABEL_EM,
        depth_em: -low / upem * LABEL_EM,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The main face is TrueType and draws quadratics; a cubic outline – a
    /// CFF face – is written as SVG `C` all the same.
    #[test]
    fn a_face_that_cannot_draw_a_label_is_a_defect() {
        assert_eq!(
            unreadable("x").to_string(),
            "the main face cannot draw a label: x"
        );
    }

    #[test]
    fn a_cubic_outline_is_written_as_a_curve() {
        let mut d = String::new();
        let mut pen = SvgPath {
            d: &mut d,
            dx: 1.0,
            low: 0.0,
            high: 0.0,
        };
        pen.move_to(0.0, 0.0);
        pen.curve_to(1.0, 2.0, 3.0, -4.0, 5.0, 6.0);
        pen.close();
        let (low, high) = (pen.low, pen.high);
        assert_eq!(d, "M 1.0 -0.0C 2.0 -2.0 4.0 4.0 6.0 -6.0Z");
        assert_eq!((low, high), (-4.0, 6.0));
    }
}
