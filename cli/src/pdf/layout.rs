//! The layout model: what a document puts on its page, before any layout
//! engine sees it (`PDF-ACCEPTANCE.md`, "The boundary to keep"). Tests read it
//! without producing a PDF.
//!
//! Sections between `parsep` and `parsep_dbl` markers, lines at `lb` (its
//! `cu` a line of its own, its `lnr` at the head of the next), a note by its
//! `c` at its anchor, `text:tab` a shift and never a character. A note's `c`
//! and a line's `cu` are markup, read as the body is ([`super::markup`]),
//! and every rule below holds in all three (`XML-CONTRACT.md` §4, "What a
//! page shows"). Comments and instructions are not set (owner's decision of
//! 2026-09-30, §5 question 3). Whitespace inside a line is layout, not text:
//! runs of it become one space, trimmed at the line's ends.

use std::collections::BTreeMap;

use crate::cth_titles::{title_text, Catalog, Match};
use crate::document::Document;
// Text reaches the template as JSON data, never as markup.
use crate::json::json_string;

use super::fonts::{Fonts, CREDITED};
use super::markup::{self, Event, Tag};
use super::PdfError;

/// A stretch of a line.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// Raised: the sign of `corr`, the letters of `materlect` (`HPMxml.css`).
    Sup(String),
    /// Lowered: the letter of `subscr` (`HPMxml.css`).
    Sub(String),
    /// A note at its anchor: its paragraphs, each of text, `Sup` and `Sub`.
    Note(Vec<Vec<Piece>>),
    /// `text:tab` of OpenDocument: a shift of fixed width.
    Tab,
}

impl Piece {
    /// The text this piece sets, in order.
    fn texts<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Piece::Text(t) | Piece::Sup(t) | Piece::Sub(t) => out.push(t),
            Piece::Note(paragraphs) => {
                for p in paragraphs.iter().flatten() {
                    p.texts(out);
                }
            }
            Piece::Tab => {}
        }
    }
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
        let mut out = vec![self.head.as_str(), self.name.as_str()];
        for p in self.sections.iter().flatten().flatten() {
            p.texts(&mut out);
        }
        out.into_iter()
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
    let events = markup::events(doc);
    let doc_id = events.iter().find_map(|e| match e {
        Event::Start(t) if t.local == "docID" => Some(t.sole_text.as_deref().unwrap_or("").trim()),
        _ => None,
    });
    let mut b = Builder::new(Where::Body, doc_id);
    for e in &events {
        b.event(e);
    }
    b.sections()
}

/// The names of tags in a note, a line's `cu` or a value a page shows that
/// no rule knows and the vocabulary does not name, with how often each
/// stands: what `check` lists as a warning (owner's decision of 2026-10-27).
pub fn unknown_names(doc: &Document<'_>) -> BTreeMap<String, usize> {
    let events = markup::events(doc);
    let mut b = Builder::new(Where::Body, None);
    for e in &events {
        b.event(e);
    }
    b.unknown
}

/// The mark between the sections of a value read as markup: U+001D, which
/// XML 1.0 does not allow in a document, so no text of one carries it.
const SECTION: char = '\u{1D}';

/// Where the markup stands: the rules of the body's structure (`lb`,
/// `parsep`, `note`, the heading's sigla, `text:tab`) hold in the body only,
/// but for `parsep` in a value the page shows; a paragraph of a note ends at
/// the end of its `P_f_…` element.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Where {
    Body,
    Note,
    Cu,
    /// The value a rule shows of an attribute: `c` of `corr`, `gap`, ….
    Value,
}

/// A stretch of a line while the document is read: a piece, or the opening
/// of an erasure whose sign is known only at its end.
enum Item {
    Piece(Piece),
    /// The `n`-th `ras_in`: `⟦`, or `(Rasur)` when nothing stands before its
    /// `ras_fin`.
    Erasure(usize),
}

/// The rules of display (`XML-CONTRACT.md` §4, "What a page shows"), applied
/// to one stream of events.
struct Builder<'d> {
    place: Where,
    doc_id: Option<&'d str>,
    sections: Vec<Vec<Vec<Item>>>,
    /// Non-whitespace code points set so far: what tells an empty erasure.
    visible: usize,
    /// Open erasures: their number, and `visible` when they opened.
    erasures: Vec<(usize, usize)>,
    /// For each erasure, whether nothing stood inside it.
    empty: Vec<bool>,
    /// Inside an `AO:TxtPubl` that repeats the `docID`: its text not set.
    skip: usize,
    /// Names of markup outside the body no rule knows, and their counts.
    unknown: BTreeMap<String, usize>,
}

impl<'d> Builder<'d> {
    fn new(place: Where, doc_id: Option<&'d str>) -> Self {
        Builder {
            place,
            doc_id,
            sections: vec![vec![Vec::new()]],
            visible: 0,
            erasures: Vec::new(),
            empty: Vec::new(),
            skip: 0,
            unknown: BTreeMap::new(),
        }
    }

    fn line(&mut self) {
        if let Some(s) = self.sections.last_mut() {
            s.push(Vec::new());
        }
    }

    fn item(&mut self, item: Item) {
        if let Some(line) = self.sections.last_mut().and_then(|s| s.last_mut()) {
            line.push(item);
        }
    }

    fn text(&mut self, t: &str) {
        self.visible += t.chars().filter(|c| !is_space(*c)).count();
        match self
            .sections
            .last_mut()
            .and_then(|s| s.last_mut())
            .and_then(|l| l.last_mut())
        {
            Some(Item::Piece(Piece::Text(s))) => s.push_str(t),
            _ => self.item(Item::Piece(Piece::Text(t.to_string()))),
        }
    }

    fn raised(&mut self, t: &str, sub: bool) {
        if t.is_empty() {
            return;
        }
        self.visible += t.chars().filter(|c| !is_space(*c)).count();
        let t = t.replace(SECTION, " ");
        self.item(Item::Piece(if sub { Piece::Sub(t) } else { Piece::Sup(t) }));
    }

    /// The text an attribute's value shows: the value as it is, or, when it
    /// carries markup, what the rules make of it, read apart from the line,
    /// its sections apart by [`SECTION`].
    fn value(&mut self, v: &str) -> String {
        if !v.contains(['<', '&', '\u{2329}']) {
            return v.to_string();
        }
        let mut b = Builder::new(Where::Value, self.doc_id);
        b.fragment(v, Where::Value);
        self.count_unknown(std::mem::take(&mut b.unknown));
        // Every section kept, an empty one too: its border is what shows.
        let empty = std::mem::take(&mut b.empty);
        b.sections
            .into_iter()
            .map(|section| {
                let mut words: Vec<String> = Vec::new();
                for line in section.into_iter().filter_map(|l| tidy(resolve(l, &empty))) {
                    for p in &line {
                        let mut t = Vec::new();
                        p.texts(&mut t);
                        words.extend(t.into_iter().map(str::to_string));
                    }
                }
                words.join(" ")
            })
            .collect::<Vec<_>>()
            .join(&SECTION.to_string())
    }

    /// A value set in the line, a `parsep` inside it a new section, as in the
    /// body (owner's decision of 2026-10-27, `CTH 456/KUB 39.52+`).
    fn shown(&mut self, v: &str) {
        for (i, part) in v.split(SECTION).enumerate() {
            if i > 0 {
                self.sections.push(vec![Vec::new()]);
            }
            self.text(part);
        }
    }

    /// Markup carried as text, read where `place` says, into this builder.
    fn fragment(&mut self, s: &str, place: Where) {
        let outer = std::mem::replace(&mut self.place, place);
        for e in &markup::fragment(s) {
            self.event(e);
        }
        self.place = outer;
    }

    fn event(&mut self, e: &Event<'_>) {
        match e {
            Event::Text(t) if self.skip == 0 => self.text(t),
            Event::Text(_) => {}
            Event::End(name) => {
                if name == "TxtPubl" && self.skip > 0 {
                    self.skip -= 1;
                }
                if self.place == Where::Note && name.starts_with("P_f_") {
                    self.line();
                }
            }
            Event::Start(tag) => self.start(tag),
        }
    }

    fn count_unknown(&mut self, names: BTreeMap<String, usize>) {
        for (k, n) in names {
            *self.unknown.entry(k).or_insert(0) += n;
        }
    }

    fn start(&mut self, tag: &Tag<'_>) {
        // A name with what is not a name in it – `<wsepc="𒀹"/>` in the `cu`
        // of `CTH 470/CHDS 7.200`: the name is its run of name characters,
        // and what follows stands as text in the tag's place, less `=`,
        // quotes, `/` and whitespace (owner's decision of 2026-10-27).
        let name_end = tag
            .local
            .find(|c: char| !markup::is_name_char(c))
            .unwrap_or(tag.local.len());
        let name = &tag.local[..name_end];
        if name_end < tag.local.len() {
            let rest: String = tag.local[name_end..]
                .chars()
                .filter(|c| !matches!(c, '=' | '"' | '\'' | '/') && !c.is_whitespace())
                .collect();
            self.text(&rest);
        }
        // What `c` shows is markup too, read on its own (`KBo 1.1+`:
        // `corr c="<ras_in/>"`).
        let raw = tag.attribute("c");
        let value = match name {
            "note" => None,
            _ => raw.map(|c| self.value(c)),
        };
        let c = value.as_deref();
        let body = self.place == Where::Body;
        match name {
            "TxtPubl" if body && tag.prefix.as_deref() == Some("AO") => {
                // Owner's decision header: a publication equal to the docID
                // is not repeated under the heading.
                if tag
                    .sole_text
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|t| Some(t) == self.doc_id)
                {
                    self.skip += 1;
                }
            }
            "lb" if body => {
                self.line();
                if let Some(cu) = tag.attribute("cu") {
                    self.fragment(cu, Where::Cu);
                    self.line();
                }
                // `lb:after { content: " " attr(lnr) " " }` (HPMxml.css).
                // Its value is markup where it carries it (`KUB 30.24+`).
                if let Some(lnr) = tag.attribute("lnr").map(|n| self.value(n)) {
                    if !lnr.is_empty() {
                        self.shown(&format!("{lnr} "));
                    }
                }
            }
            "tab" if body && tag.prefix.as_deref() == Some("text") => {
                self.item(Item::Piece(Piece::Tab));
            }
            "parsep" | "parsep_dbl" if body || self.place == Where::Value => {
                self.sections.push(vec![Vec::new()]);
            }
            "note" if body => {
                if let Some(c) = raw {
                    let mut note = Builder::new(Where::Note, self.doc_id);
                    note.fragment(c, Where::Note);
                    self.count_unknown(std::mem::take(&mut note.unknown));
                    let paragraphs: Vec<Vec<Piece>> =
                        note.sections().into_iter().flatten().collect();
                    if !paragraphs.is_empty() {
                        self.item(Item::Piece(Piece::Note(paragraphs)));
                    }
                }
            }
            // HPMxml.css: del_in::before "[", del_fin::after "]",
            // laes_in::before "⸢", laes_fin::after "⸣"; owner's decision C5.
            "del_in" => self.text("["),
            "del_fin" => self.text("]"),
            "laes_in" => self.text("\u{2E22}"),
            "laes_fin" => self.text("\u{2E23}"),
            "add_in" => self.text("\u{2329}"),
            "add_fin" => self.text("\u{232A}"),
            "corr" | "materlect" => self.raised(c.unwrap_or(""), false),
            "subscr" => self.raised(c.unwrap_or(""), true),
            "surpl" => {
                if let Some(c) = c {
                    self.shown(&format!("\u{2329}\u{2329}{c}\u{232A}\u{232A}"));
                }
            }
            // Owner's decisions of 2026-10-27, C1, C2, C3 and C7.
            "wsep" => self.shown(c.unwrap_or("")),
            "gap" if tag.attribute("t") == Some("line") => {
                self.line();
                self.shown(c.unwrap_or(""));
                self.line();
            }
            "gap" => self.shown(c.unwrap_or("")),
            "ParagrNr" => {
                self.line();
                self.shown(c.unwrap_or(""));
                self.line();
            }
            "DirectJoin" => self.text(" + "),
            "InDirectJoin" => self.text(" (+) "),
            // C4: an erasure in ⟦ ⟧, an empty one as (Rasur).
            "ras_in" => {
                let n = self.empty.len();
                self.empty.push(false);
                self.visible += 1;
                self.item(Item::Erasure(n));
                self.erasures.push((n, self.visible));
            }
            "ras_fin" => match self.erasures.pop() {
                Some((n, seen)) if seen == self.visible => {
                    if let Some(e) = self.empty.get_mut(n) {
                        *e = true;
                    }
                }
                _ => self.text("\u{27E7}"),
            },
            "ras_X" => self.text("(Rasur)"),
            // Every other tag – QUOT_HurInHit (C6), the wrappers of a note,
            // a name no rule knows – is not set; what it holds is. Outside
            // the body a name the vocabulary does not name is counted.
            _ => {
                if !body {
                    let qualified = match tag.prefix.as_deref() {
                        Some(p) => format!("{p}:{}", tag.local),
                        None => tag.local.to_string(),
                    };
                    if !markup::VOCABULARY.contains(&qualified.as_str()) {
                        *self.unknown.entry(qualified).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    /// The sections, erasures resolved and whitespace settled.
    fn sections(self) -> Vec<Section> {
        let empty = self.empty;
        self.sections
            .into_iter()
            .map(|section| {
                section
                    .into_iter()
                    .filter_map(|line| tidy(resolve(line, &empty)))
                    .collect::<Section>()
            })
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// A line's items as pieces: each erasure its sign, neighbouring text joined.
fn resolve(line: Vec<Item>, empty: &[bool]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(line.len());
    for item in line {
        let piece = match item {
            Item::Piece(p) => p,
            Item::Erasure(n) => Piece::Text(
                if empty.get(n).copied().unwrap_or(false) {
                    "(Rasur)"
                } else {
                    "\u{27E6}"
                }
                .to_string(),
            ),
        };
        match (out.last_mut(), piece) {
            (Some(Piece::Text(s)), Piece::Text(t)) => s.push_str(&t),
            (_, p) => out.push(p),
        }
    }
    out
}

/// One line with its whitespace settled: text to one space at the edges,
/// trimmed at the line's ends, a tab at the end dropped – it shifts nothing
/// after it – and an empty line gone.
fn tidy(line: Vec<Piece>) -> Option<Vec<Piece>> {
    let mut l: Vec<Piece> = line
        .into_iter()
        .map(|p| match p {
            Piece::Text(t) => Piece::Text(edges(&t)),
            Piece::Sup(t) => Piece::Sup(one_space(&t)),
            Piece::Sub(t) => Piece::Sub(one_space(&t)),
            other => other,
        })
        .collect();
    if let Some(Piece::Text(t)) = l.first_mut() {
        *t = t.trim_start().to_string();
    }
    if let Some(Piece::Text(t)) = l.last_mut() {
        *t = t.trim_end().to_string();
    }
    l.retain(|p| !matches!(p, Piece::Text(t) | Piece::Sup(t) | Piece::Sub(t) if t.is_empty()));
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

/// The pieces of a line as JSON items, without the brackets.
fn line_items(fonts: &Fonts, line: &[Piece]) -> Result<Vec<String>, PdfError> {
    let mut items = Vec::new();
    for piece in line {
        match piece {
            Piece::Text(t) => items.extend(runs_items(fonts, t)?),
            Piece::Sup(t) => {
                items.push(format!("{{\"k\":\"sup\",\"r\":{}}}", runs_json(fonts, t)?))
            }
            Piece::Sub(t) => {
                items.push(format!("{{\"k\":\"sub\",\"r\":{}}}", runs_json(fonts, t)?))
            }
            Piece::Note(paragraphs) => {
                let p = paragraphs
                    .iter()
                    .map(|l| Ok(format!("[{}]", line_items(fonts, l)?.join(","))))
                    .collect::<Result<Vec<_>, PdfError>>()?;
                items.push(format!("{{\"k\":\"n\",\"p\":[{}]}}", p.join(",")));
            }
            Piece::Tab => items.push("{\"k\":\"tab\"}".to_string()),
        }
    }
    Ok(items)
}

/// The data of the template, `/doc.json`.
pub fn json(fonts: &Fonts, page: &Page, template: &str, credit: bool) -> Result<String, PdfError> {
    let mut sections = Vec::with_capacity(page.sections.len());
    for section in &page.sections {
        let mut lines = Vec::with_capacity(section.len());
        for line in section {
            lines.push(format!("[{}]", line_items(fonts, line)?.join(",")));
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
