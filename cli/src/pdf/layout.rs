//! The layout model: what a document puts on its page, before any layout
//! engine sees it (`PDF-ACCEPTANCE.md`, "The boundary to keep"). Tests read it
//! without producing a PDF.
//!
//! Sections between `parsep` and `parsep_dbl` markers, lines at `lb` (its
//! `cu` a line of its own), a note by its `c` at its anchor, `text:tab` a
//! shift and never a character. Comments, instructions and every other
//! attribute are not set (owner's decision of 2026-09-30 on comments,
//! `XML-CONTRACT.md` §5 question 3). Whitespace inside a line is layout, not
//! text: runs of it become one space, trimmed at the line's ends.

use crate::cth_titles::{title_text, Catalog, Match};
use crate::document::{Document, Kind};
// Text reaches the template as JSON data, never as markup.
use crate::json::json_string;

use super::fonts::{Fonts, CREDITED};
use super::PdfError;

/// A stretch of a line.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// A note, by its `c`, at its anchor.
    Note(String),
    /// `text:tab` of OpenDocument: a shift of fixed width.
    Tab,
}

/// A section: lines of pieces.
pub type Section = Vec<Vec<Piece>>;

/// What one document puts on its page.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    /// Folder CTH and its title.
    pub head: String,
    /// The file name, the siglum.
    pub name: String,
    pub sections: Vec<Section>,
}

impl Page {
    /// The page of a document of group `group` (the folder, `CTH 746`) and
    /// name `name` (the siglum, the file name without `.xml`).
    pub fn of(doc: &Document<'_>, group: &str, name: &str) -> Page {
        Page {
            head: format!("{group} – {}", title_for(group)),
            name: name.to_string(),
            sections: sections(doc),
        }
    }

    /// Every stretch of text the page sets, in page order: heading, name,
    /// then each line's text and notes.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        let body = self
            .sections
            .iter()
            .flatten()
            .flatten()
            .filter_map(|p| match p {
                Piece::Text(t) | Piece::Note(t) => Some(t.as_str()),
                Piece::Tab => None,
            });
        [self.head.as_str(), self.name.as_str()]
            .into_iter()
            .chain(body)
    }

    /// Whether any face of the stack that draws this page is UllikummiA:
    /// the prediction of the credit, made from the fonts before Typst runs.
    pub fn uses_credited_face(&self, fonts: &Fonts) -> bool {
        self.texts()
            .flat_map(str::chars)
            .any(|c| fonts.face(u32::from(c)) == Some(CREDITED))
    }

    /// The code points no face of the stack draws, in page order: one label
    /// each.
    pub fn labels(&self, fonts: &Fonts) -> Vec<u32> {
        self.texts()
            .flat_map(|t| runs(fonts, t))
            .filter_map(|r| match r {
                Run::Label(cp) => Some(cp),
                _ => None,
            })
            .collect()
    }

    /// Whether the page carries a combining mark of U+0300–U+036F: the one
    /// case where Typst 0.15.1 may read a cluster twice (typst/typst #4225).
    pub fn has_combining_marks(&self) -> bool {
        self.texts().flat_map(str::chars).any(combining)
    }
}

/// A combining mark of the block U+0300–U+036F.
pub(crate) fn combining(c: char) -> bool {
    ('\u{0300}'..='\u{036F}').contains(&c)
}

/// The title the catalog of CTH gives a group, or nothing.
fn title_for(group: &str) -> String {
    match Catalog::compiled().lookup(Some(group)) {
        Match::Exact(t) => title_text(t),
        Match::Parent { title, .. } => title_text(title),
        _ => String::new(),
    }
}

/// Whitespace as a line keeps it: runs of space, tab, CR and LF to one space.
pub(crate) fn one_space(s: &str) -> String {
    s.split([' ', '\t', '\n', '\r'])
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// Whitespace to one space, kept at the edges of a piece of text so that the
/// words either side of a note or a tab stay apart.
fn edges(t: &str) -> String {
    let inner = one_space(t);
    if inner.is_empty() {
        return if t.is_empty() {
            String::new()
        } else {
            " ".into()
        };
    }
    let lead = if t.starts_with(is_space) { " " } else { "" };
    let tail = if t.ends_with(is_space) { " " } else { "" };
    format!("{lead}{inner}{tail}")
}

/// The sections of a document, in document order.
pub fn sections(doc: &Document<'_>) -> Vec<Section> {
    let mut out: Vec<Section> = vec![vec![Vec::new()]];
    for node in doc.nodes() {
        // `out` is never empty and neither is its last section: both are
        // pushed with a first member.
        let Some(section) = out.last_mut() else { break };
        match &node.kind {
            Kind::Element(e) if e.name.local == "lb" => {
                section.push(Vec::new());
                if let Some(cu) = e.attributes.iter().find(|a| a.name.local == "cu") {
                    section.push(vec![Piece::Text(cu.value.to_string())]);
                    section.push(Vec::new());
                }
            }
            Kind::Element(e)
                if e.name.local == "tab" && e.name.prefix.as_deref() == Some("text") =>
            {
                push(section, Piece::Tab);
            }
            Kind::Element(e) if e.name.local == "parsep" || e.name.local == "parsep_dbl" => {
                out.push(vec![Vec::new()]);
            }
            Kind::Element(e) if e.name.local == "note" => {
                if let Some(c) = e.attributes.iter().find(|a| a.name.local == "c") {
                    push(section, Piece::Note(c.value.to_string()));
                }
            }
            Kind::Text(t) => match section.last_mut().and_then(|l| l.last_mut()) {
                Some(Piece::Text(s)) => s.push_str(t),
                _ => push(section, Piece::Text(t.to_string())),
            },
            _ => {}
        }
    }
    out.into_iter()
        .map(|section| section.into_iter().filter_map(tidy).collect::<Section>())
        .filter(|s| !s.is_empty())
        .collect()
}

fn push(section: &mut Section, piece: Piece) {
    match section.last_mut() {
        Some(line) => line.push(piece),
        None => section.push(vec![piece]),
    }
}

/// One line with its whitespace settled: text to one space at the edges,
/// trimmed at the line's ends, a tab at the end dropped – it shifts nothing
/// after it – and an empty line gone.
fn tidy(line: Vec<Piece>) -> Option<Vec<Piece>> {
    let mut l: Vec<Piece> = line
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => Piece::Text(edges(&t)),
            Piece::Note(c) => Piece::Note(one_space(&c)),
            Piece::Tab => Piece::Tab,
        })
        .collect();
    if let Some(Piece::Text(t)) = l.first_mut() {
        *t = t.trim_start().to_string();
    }
    if let Some(Piece::Text(t)) = l.last_mut() {
        *t = t.trim_end().to_string();
    }
    l.retain(|p| !matches!(p, Piece::Text(t) | Piece::Note(t) if t.is_empty()));
    while l.last() == Some(&Piece::Tab) {
        l.pop();
    }
    if let Some(Piece::Text(t)) = l.last_mut() {
        *t = t.trim_end().to_string();
    }
    l.retain(|p| !matches!(p, Piece::Text(t) if t.is_empty()));
    (!l.is_empty()).then_some(l)
}

/// A stretch of one line as the template sets it.
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    Text(String),
    /// Signs of a run with no break opportunity: cuneiform (Noto Sans
    /// Cuneiform, UllikummiA) and `▒`, boxed sixteen at a time.
    Series(String),
    /// A code point no face of the stack draws.
    Label(u32),
}

/// Whether a code point needs no glyph: the whitespace the lines keep.
fn needs_no_glyph(cp: u32) -> bool {
    matches!(cp, 0x09 | 0x0A | 0x0D | 0x20)
}

fn is_series(fonts: &Fonts, cp: u32) -> bool {
    cp == 0x2592 || matches!(fonts.face(cp), Some("Noto Sans Cuneiform" | "UllikummiA"))
}

/// A stretch of text as runs: which code points are labels is read from the
/// fonts, never from a list.
pub fn runs(fonts: &Fonts, line: &str) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for c in line.chars() {
        let cp = u32::from(c);
        if !needs_no_glyph(cp) && fonts.face(cp).is_none() {
            out.push(Run::Label(cp));
            continue;
        }
        let series = is_series(fonts, cp);
        match out.last_mut() {
            Some(Run::Series(s)) if series => s.push(c),
            Some(Run::Text(s)) if !series => s.push(c),
            _ => out.push(if series {
                Run::Series(c.to_string())
            } else {
                Run::Text(c.to_string())
            }),
        }
    }
    out
}

/// The runs of a stretch as JSON items, without the brackets.
fn runs_items(fonts: &Fonts, text: &str) -> Result<Vec<String>, PdfError> {
    runs(fonts, text)
        .iter()
        .map(|r| {
            Ok(match r {
                Run::Text(s) => format!("{{\"k\":\"t\",\"s\":{}}}", json_string(s)),
                Run::Series(s) => format!("{{\"k\":\"s\",\"s\":{}}}", json_string(s)),
                Run::Label(cp) => {
                    let art = fonts.art(*cp)?;
                    format!(
                        "{{\"k\":\"u\",\"svg\":{},\"h\":{:.4},\"d\":{:.4}}}",
                        json_string(&art.svg),
                        art.height_em,
                        art.depth_em
                    )
                }
            })
        })
        .collect()
}

fn runs_json(fonts: &Fonts, text: &str) -> Result<String, PdfError> {
    Ok(format!("[{}]", runs_items(fonts, text)?.join(",")))
}

/// The data of the template, `/doc.json`.
pub fn json(fonts: &Fonts, page: &Page, template: &str, credit: bool) -> Result<String, PdfError> {
    let mut sections = Vec::with_capacity(page.sections.len());
    for section in &page.sections {
        let mut lines = Vec::with_capacity(section.len());
        for line in section {
            let mut items = Vec::new();
            for piece in line {
                match piece {
                    Piece::Text(t) => items.extend(runs_items(fonts, t)?),
                    Piece::Note(c) => {
                        items.push(format!("{{\"k\":\"n\",\"r\":{}}}", runs_json(fonts, c)?))
                    }
                    Piece::Tab => items.push("{\"k\":\"tab\"}".to_string()),
                }
            }
            lines.push(format!("[{}]", items.join(",")));
        }
        sections.push(format!("[{}]", lines.join(",")));
    }
    Ok(format!(
        "{{\"template\":{},\"plain-name\":{},\"head\":{},\"name\":{},\"credit\":{},\"sections\":[{}]}}",
        json_string(template),
        json_string(&page.name),
        runs_json(fonts, &page.head)?,
        runs_json(fonts, &page.name)?,
        if credit {
            json_string(crate::fonts::CREDIT)
        } else {
            "null".into()
        },
        sections.join(",")
    ))
}
