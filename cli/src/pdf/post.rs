//! What is done to a PDF after Typst, with `lopdf`, and the one walk over a
//! page's content stream it rests on.
//!
//! Two patches, done in one load and one save by [`finish`], and only to a
//! file that needs one – every other file leaves Typst byte for byte:
//!
//! - the labels, variant B of the owner's decisions of 2026-09-27 and
//!   2026-09-28: a label is an image Typst draws from the outlines of the main
//!   face, so no reader extracts its letters, and this module writes the code
//!   point it stands for as invisible text – render mode 3, a Type3 font whose
//!   glyphs are empty, with a `ToUnicode` entry – into the image's own
//!   marked-content span. The Type3 font embeds no outline and is not a face
//!   of the stack;
//! - the clusters, variant A of the owner's decision of 2026-09-27: where
//!   Typst 0.15.1 reads a cluster twice (typst/typst #4225), the two pieces
//!   are wrapped in one `/Span` with the `/ActualText` of the first.
//!
//! The walk ([`scan_page`]) also serves the checks of the tests and of the
//! gate, which read what it yields: where each glyph and path point lands,
//! which glyph codes are shown (glyph 0 is `.notdef`), and what text a reader
//! gets with and without `/ActualText`.

use std::collections::{BTreeMap, HashMap};

use lopdf::{Document, Object, ObjectId};

use super::{Invariant, PdfError};

/// A `lopdf` failure on a file this program just wrote: not the document's
/// fault, so an invariant.
fn broken(e: impl std::fmt::Display) -> PdfError {
    PdfError::Invariant(Invariant::Pdf(e.to_string()))
}

/// A font resource of a page: what each two-byte code means and how wide it is.
#[derive(Default)]
struct FontInfo {
    base: String,
    to_unicode: HashMap<u16, String>,
    widths: HashMap<u16, f32>,
    default_width: f32,
    /// Bytes per code in a string: two for the CID fonts Typst writes, one
    /// for a simple font such as the Type3 font of the invisible layer.
    code_bytes: usize,
    /// Glyph space to text space: 1/1000 except where a Type3 `FontMatrix`
    /// says otherwise.
    scale: f32,
}

/// One glyph as shown: where it starts and ends on the page (device space,
/// points, origin bottom left) and the baseline it sits on.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub code: u16,
    pub font: String,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    /// What the glyph's ToUnicode entry says.
    pub text: String,
}

/// One marked-content span of the page with the text shown inside it.
#[derive(Debug)]
pub struct Span {
    pub mcid: Option<i64>,
    pub tag: String,
    pub actual_text: Option<String>,
    /// Text of the glyphs inside, through ToUnicode.
    pub text: String,
    /// The fill colour in force at the first glyph inside, if any glyph.
    pub fill: Option<Vec<f32>>,
    /// The transformation in force where the span opens.
    pub ctm: [f32; 6],
    /// The first clipping path set inside, as a box in device space: an
    /// image Typst draws through krilla-svg is clipped to its own frame, and
    /// only an image is, in this template.
    pub clip: Option<[f32; 4]>,
    /// Glyphs shown inside.
    pub glyphs: usize,
    /// The span this one opens inside, if any.
    pub parent: Option<usize>,
    /// Index of the `BDC` and of the matching `EMC` among the page's operations.
    pub open_op: usize,
    pub close_op: Option<usize>,
    /// What a reader that honours `/ActualText` gets from this span: glyph
    /// text, with the ActualText of a replacing span inside it instead of
    /// that span's glyphs. An outer replacing span without an MCID gives its
    /// ActualText to the first MCID span opened inside it.
    pub resolved: String,
}

/// Everything one walk over one page yields.
#[derive(Default)]
pub struct PageScan {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
    /// Device-space points of every path (label frames, rules).
    pub path_points: Vec<(f32, f32)>,
    pub spans: Vec<Span>,
    /// Text as a reader that honours `/ActualText` gets it.
    pub text: String,
    /// Text as a reader that ignores marked content gets it (glyphs only).
    pub raw_text: String,
    /// Glyph 0 shown, by font base name.
    pub notdef: BTreeMap<String, usize>,
    /// Base names of the fonts the page's resources carry.
    pub fonts: Vec<String>,
}

fn hex_units(h: &str) -> String {
    let units: Vec<u16> = (0..h.len() / 4)
        .filter_map(|i| u16::from_str_radix(&h[i * 4..i * 4 + 4], 16).ok())
        .collect();
    char::decode_utf16(units).filter_map(Result::ok).collect()
}

/// `bfchar` and `bfrange` of a ToUnicode CMap.
fn parse_to_unicode(cmap: &str) -> HashMap<u16, String> {
    let mut out = HashMap::new();
    let tokens: Vec<&str> = cmap.split_whitespace().collect();
    let hex = |t: &str| t.trim_start_matches('<').trim_end_matches('>').to_string();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "beginbfchar" {
            i += 1;
            while i + 1 < tokens.len() && tokens[i] != "endbfchar" {
                if let Ok(cid) = u16::from_str_radix(&hex(tokens[i]), 16) {
                    out.insert(cid, hex_units(&hex(tokens[i + 1])));
                }
                i += 2;
            }
        } else if tokens[i] == "beginbfrange" {
            i += 1;
            while i + 2 < tokens.len() && tokens[i] != "endbfrange" {
                let lo = u16::from_str_radix(&hex(tokens[i]), 16).unwrap_or(1);
                let hi = u16::from_str_radix(&hex(tokens[i + 1]), 16).unwrap_or(0);
                let start: Vec<char> = hex_units(&hex(tokens[i + 2])).chars().collect();
                for (k, cid) in (lo..=hi).enumerate() {
                    let mut s = start.clone();
                    if let Some(last) = s.last_mut() {
                        *last = char::from_u32(u32::from(*last) + k as u32).unwrap_or(*last);
                    }
                    out.insert(cid, s.into_iter().collect());
                }
                i += 3;
            }
        }
        i += 1;
    }
    out
}

fn number(o: &Object) -> f32 {
    match o {
        Object::Integer(i) => *i as f32,
        Object::Real(r) => *r,
        _ => 0.0,
    }
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

fn font_info(doc: &Document, dict: &lopdf::Dictionary) -> FontInfo {
    let mut info = FontInfo {
        default_width: 1000.0,
        code_bytes: 2,
        scale: 0.001,
        ..FontInfo::default()
    };
    let subtype = dict
        .get(b"Subtype")
        .and_then(|o| o.as_name())
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    if subtype != b"Type0" {
        simple_font(doc, dict, &subtype, &mut info);
    }
    info.base = dict
        .get(b"BaseFont")
        .and_then(|o| o.as_name())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    if let Ok(stream) = dict
        .get(b"ToUnicode")
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_stream())
    {
        let content = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        info.to_unicode = parse_to_unicode(&String::from_utf8_lossy(&content));
    }
    let descendant = dict
        .get(b"DescendantFonts")
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array())
        .ok()
        .and_then(|a| a.first())
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok());
    if let Some(cid) = descendant {
        cid_widths(doc, cid, &mut info);
    }
    info
}

/// A simple font – here the Type3 font of the invisible layer: one byte per
/// code, its own `FontMatrix`, widths from `FirstChar` on.
fn simple_font(doc: &Document, dict: &lopdf::Dictionary, subtype: &[u8], info: &mut FontInfo) {
    info.code_bytes = 1;
    info.default_width = 0.0;
    if subtype == b"Type3" {
        if let Ok(m) = dict.get(b"FontMatrix").and_then(|o| o.as_array()) {
            info.scale = m.first().map_or(0.001, number);
        }
    }
    let first = dict.get(b"FirstChar").map_or(0.0, number) as u32;
    if let Ok(w) = dict
        .get(b"Widths")
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array())
    {
        for (k, v) in w.iter().enumerate() {
            info.widths
                .insert((first + k as u32) as u16, number(deref(doc, v)));
        }
    }
}

/// The widths of a CID font: `DW`, and `W` in both of its forms – a first
/// code with a list, or a range with one width.
fn cid_widths(doc: &Document, cid: &lopdf::Dictionary, info: &mut FontInfo) {
    if let Ok(dw) = cid.get(b"DW") {
        info.default_width = number(deref(doc, dw));
    }
    let Ok(w) = cid
        .get(b"W")
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array())
    else {
        return;
    };
    let mut i = 0;
    while i < w.len() {
        let first = number(deref(doc, &w[i])) as u32;
        match w.get(i + 1).map(|o| deref(doc, o)) {
            Some(Object::Array(list)) => {
                for (k, v) in list.iter().enumerate() {
                    info.widths
                        .insert((first + k as u32) as u16, number(deref(doc, v)));
                }
                i += 2;
            }
            Some(last) => {
                let last = number(last) as u32;
                let v = w.get(i + 2).map_or(0.0, |o| number(deref(doc, o)));
                for c in first..=last {
                    info.widths.insert(c as u16, v);
                }
                i += 3;
            }
            None => break,
        }
    }
}

type M = [f32; 6];

fn mul(a: M, b: M) -> M {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: M, x: f32, y: f32) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

const ID: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn matrix(ops: &[Object]) -> M {
    let mut m = ID;
    for (k, o) in ops.iter().take(6).enumerate() {
        m[k] = number(o);
    }
    m
}

/// The text state parameters (ISO 32000 §9.3), which `q` saves with the rest
/// of the graphics state (§8.4.1): `Tf`, `Tc`, `Tz`, `TL`, `Ts`.
#[derive(Clone)]
struct TextState {
    font: Vec<u8>,
    size: f32,
    tc: f32,
    tz: f32,
    tl: f32,
    rise: f32,
}

/// The state of one walk over one page, and what it has found so far.
struct Walk {
    scan: PageScan,
    fonts: HashMap<Vec<u8>, FontInfo>,
    ctm: M,
    fill: Vec<f32>,
    text: TextState,
    saved: Vec<(M, Vec<f32>, TextState)>,
    tm: M,
    tlm: M,
    /// Marked content: index into `scan.spans` for spans, None for others,
    /// and whether ActualText of an enclosing span replaces the glyphs.
    mc: Vec<(Option<usize>, bool)>,
    /// The path under construction, in device space.
    path: Vec<(f32, f32)>,
    /// ActualText of a replacing span with no MCID, waiting for the first
    /// MCID span inside it.
    pending: Option<String>,
}

/// Walks one page. `lopdf` decodes the operators; the arithmetic of the text
/// and graphics state is the part of ISO 32000 §9.4 this output uses: `cm`,
/// `q`/`Q`, `BT`/`ET`, `Tm`, `Td`, `TD`, `T*`, `TL`, `Tf`, `Tc`, `Tz`, `Ts`,
/// `Tj`, `TJ`, paths by `m`, `l`, `c`, `re`, and marked content.
pub fn scan_page(doc: &Document, page: ObjectId) -> Result<PageScan, PdfError> {
    let mut scan = PageScan::default();
    let page_dict = doc.get_dictionary(page).map_err(broken)?;
    let media = page_dict
        .get(b"MediaBox")
        .and_then(|o| o.as_array())
        .map(|a| a.iter().map(number).collect::<Vec<_>>())
        .unwrap_or_else(|_| vec![0.0, 0.0, 595.28, 841.89]);
    scan.width = media.get(2).copied().unwrap_or(0.0);
    scan.height = media.get(3).copied().unwrap_or(0.0);

    let mut fonts: HashMap<Vec<u8>, FontInfo> = HashMap::new();
    for (name, dict) in doc.get_page_fonts(page).unwrap_or_default() {
        let info = font_info(doc, dict);
        scan.fonts.push(info.base.clone());
        fonts.insert(name, info);
    }

    let content = doc.get_page_content(page);
    let ops = lopdf::content::Content::decode(&content)
        .map(|c| c.operations)
        .unwrap_or_default();

    let mut walk = Walk::new(scan, fonts);
    for (index, op) in ops.iter().enumerate() {
        walk.step(index, op);
    }
    Ok(walk.scan)
}

impl Walk {
    fn new(scan: PageScan, fonts: HashMap<Vec<u8>, FontInfo>) -> Self {
        Walk {
            scan,
            fonts,
            ctm: ID,
            fill: vec![0.0],
            text: TextState {
                font: Vec::new(),
                size: 0.0,
                tc: 0.0,
                tz: 100.0,
                tl: 0.0,
                rise: 0.0,
            },
            saved: Vec::new(),
            tm: ID,
            tlm: ID,
            mc: Vec::new(),
            path: Vec::new(),
            pending: None,
        }
    }

    fn step(&mut self, index: usize, op: &lopdf::content::Operation) {
        let a = &op.operands;
        match op.operator.as_str() {
            "q" => self
                .saved
                .push((self.ctm, self.fill.clone(), self.text.clone())),
            "Q" => {
                if let Some((ctm, fill, text)) = self.saved.pop() {
                    self.ctm = ctm;
                    self.fill = fill;
                    self.text = text;
                }
            }
            "cm" => self.ctm = mul(matrix(a), self.ctm),
            "sc" | "scn" | "g" | "rg" | "k" => {
                self.fill = a
                    .iter()
                    .filter(|o| !matches!(o, Object::Name(_)))
                    .map(number)
                    .collect()
            }
            "BT" | "Tm" | "Td" | "TD" | "T*" | "TL" | "Tc" | "Tz" | "Ts" | "Tf" => {
                self.text_operator(op.operator.as_str(), a);
            }
            "Tj" => self.show(a.iter().collect()),
            "TJ" => self.show(
                a.first()
                    .and_then(|o| o.as_array().ok())
                    .map(|v| v.iter().collect())
                    .unwrap_or_default(),
            ),
            "m" | "l" | "c" | "re" => self.path_operator(op.operator.as_str(), a),
            "BMC" | "BDC" => self.open_span(index, a),
            "EMC" => {
                if let Some((Some(s), _)) = self.mc.pop() {
                    self.scan.spans[s].close_op = Some(index);
                }
            }
            "W" | "W*" => self.clip(),
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => self.path.clear(),
            _ => {}
        }
    }

    /// The text object and the text state: `BT`, `Tm`, `Td`, `TD`, `T*`,
    /// `TL`, `Tc`, `Tz`, `Ts`, `Tf`.
    fn text_operator(&mut self, operator: &str, a: &[Object]) {
        match operator {
            "BT" => {
                self.tm = ID;
                self.tlm = ID;
            }
            "Tm" => {
                self.tm = matrix(a);
                self.tlm = self.tm;
            }
            "Td" | "TD" => {
                let (tx, ty) = (a.first().map_or(0.0, number), a.get(1).map_or(0.0, number));
                if operator == "TD" {
                    self.text.tl = -ty;
                }
                self.tlm = mul([1.0, 0.0, 0.0, 1.0, tx, ty], self.tlm);
                self.tm = self.tlm;
            }
            "T*" => {
                self.tlm = mul([1.0, 0.0, 0.0, 1.0, 0.0, -self.text.tl], self.tlm);
                self.tm = self.tlm;
            }
            "TL" => self.text.tl = a.first().map_or(0.0, number),
            "Tc" => self.text.tc = a.first().map_or(0.0, number),
            "Tz" => self.text.tz = a.first().map_or(100.0, number),
            "Ts" => self.text.rise = a.first().map_or(0.0, number),
            "Tf" => {
                self.text.font = a
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .unwrap_or_default()
                    .to_vec();
                self.text.size = a.get(1).map_or(0.0, number);
            }
            _ => {}
        }
    }

    /// `Tj` and `TJ`: strings are shown glyph by glyph, numbers move the
    /// text matrix back by thousandths of the font size.
    fn show(&mut self, items: Vec<&Object>) {
        let replaced = self.mc.iter().any(|(_, r)| *r);
        let open_span = self.mc.iter().rev().find_map(|(s, _)| *s);
        for item in items {
            match item {
                Object::String(bytes, _) => self.show_string(bytes, replaced, open_span),
                other => {
                    let tx = -number(other) / 1000.0 * self.text.size * self.text.tz / 100.0;
                    self.tm = mul([1.0, 0.0, 0.0, 1.0, tx, 0.0], self.tm);
                }
            }
        }
    }

    /// One string of `Tj` or `TJ`, code by code: where each glyph lands, what
    /// it reads as, and to which text and spans that reading goes.
    fn show_string(&mut self, bytes: &[u8], replaced: bool, open_span: Option<usize>) {
        let info = self.fonts.get(&self.text.font);
        let (size, tc, tz, rise) = (self.text.size, self.text.tc, self.text.tz, self.text.rise);
        let width = info.map_or(2, |f| f.code_bytes);
        for pair in bytes.chunks(width) {
            let code = if pair.len() == 2 {
                u16::from_be_bytes([pair[0], pair[1]])
            } else {
                u16::from(pair[0])
            };
            let w = info
                .and_then(|f| f.widths.get(&code).copied())
                .unwrap_or_else(|| info.map_or(1000.0, |f| f.default_width))
                * info.map_or(0.001, |f| f.scale);
            let trm = mul(
                [size * tz / 100.0, 0.0, 0.0, size, 0.0, rise],
                mul(self.tm, self.ctm),
            );
            let (x0, y) = apply(trm, 0.0, 0.0);
            let (x1, _) = apply(trm, w, 0.0);
            let base = info.map_or_else(String::new, |f| f.base.clone());
            if code == 0 && pair.len() == 2 {
                *self.scan.notdef.entry(base.clone()).or_insert(0) += 1;
            }
            let text = info
                .and_then(|f| f.to_unicode.get(&code).cloned())
                .unwrap_or_default();
            self.scan.raw_text.push_str(&text);
            if !replaced {
                self.scan.text.push_str(&text);
                for s in self.mc.iter().filter_map(|(s, _)| *s) {
                    self.scan.spans[s].resolved.push_str(&text);
                }
            }
            if let Some(s) = open_span {
                let span = &mut self.scan.spans[s];
                span.glyphs += 1;
                span.text.push_str(&text);
                if span.fill.is_none() {
                    span.fill = Some(self.fill.clone());
                }
            }
            self.scan.glyphs.push(Glyph {
                code,
                font: base,
                x0: x0.min(x1),
                x1: x0.max(x1),
                y,
                text,
            });
            let tx = (w * size + tc) * tz / 100.0;
            self.tm = mul([1.0, 0.0, 0.0, 1.0, tx, 0.0], self.tm);
        }
    }

    /// `m`, `l`, `c`, `re`: the points of the path, in device space.
    fn path_operator(&mut self, operator: &str, a: &[Object]) {
        let mut points = Vec::new();
        match operator {
            "m" | "l" => points.push(apply(
                self.ctm,
                a.first().map_or(0.0, number),
                a.get(1).map_or(0.0, number),
            )),
            "c" => {
                for k in 0..3 {
                    points.push(apply(
                        self.ctm,
                        a.get(2 * k).map_or(0.0, number),
                        a.get(2 * k + 1).map_or(0.0, number),
                    ));
                }
            }
            _ => {
                let v: Vec<f32> = a.iter().map(number).collect();
                if v.len() == 4 {
                    for (x, y) in [(v[0], v[1]), (v[0] + v[2], v[1] + v[3])] {
                        points.push(apply(self.ctm, x, y));
                    }
                }
            }
        }
        for p in points {
            self.scan.path_points.push(p);
            self.path.push(p);
        }
    }

    /// `BMC` and `BDC`: a span, its MCID and ActualText, and where that
    /// ActualText goes in the text a reader gets.
    fn open_span(&mut self, index: usize, a: &[Object]) {
        let tag = a
            .first()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_default();
        let props = a.get(1).and_then(|o| o.as_dict().ok());
        let mcid = props
            .and_then(|d| d.get(b"MCID").ok())
            .and_then(|o| o.as_i64().ok());
        let actual = props
            .and_then(|d| d.get(b"ActualText").ok())
            .and_then(|o| o.as_str().ok())
            .map(decode_text_string);
        let outer_replaced = self.mc.iter().any(|(_, r)| *r);
        if let Some(t) = &actual {
            if !outer_replaced {
                self.scan.text.push_str(t);
                for s in self.mc.iter().filter_map(|(s, _)| *s) {
                    self.scan.spans[s].resolved.push_str(t);
                }
                if mcid.is_none()
                    && !self
                        .mc
                        .iter()
                        .any(|(s, _)| s.is_some_and(|s| self.scan.spans[s].mcid.is_some()))
                {
                    self.pending = Some(t.clone());
                }
            }
        }
        self.scan.spans.push(Span {
            mcid,
            tag,
            actual_text: actual.clone(),
            text: String::new(),
            fill: None,
            ctm: self.ctm,
            clip: None,
            glyphs: 0,
            parent: self.mc.iter().rev().find_map(|(s, _)| *s),
            open_op: index,
            close_op: None,
            resolved: String::new(),
        });
        if mcid.is_some() {
            if let Some(t) = self.pending.take() {
                let last = self.scan.spans.len() - 1;
                self.scan.spans[last].resolved.push_str(&t);
            }
        }
        self.mc
            .push((Some(self.scan.spans.len() - 1), actual.is_some()));
    }

    /// `W` and `W*`: the first clipping path inside the innermost span, as a
    /// box.
    fn clip(&mut self) {
        if let (Some(s), false) = (
            self.mc.iter().rev().find_map(|(s, _)| *s),
            self.path.is_empty(),
        ) {
            if self.scan.spans[s].clip.is_none() {
                let xs = self.path.iter().map(|p| p.0);
                let ys = self.path.iter().map(|p| p.1);
                self.scan.spans[s].clip = Some([
                    xs.clone().fold(f32::MAX, f32::min),
                    ys.clone().fold(f32::MAX, f32::min),
                    xs.fold(f32::MIN, f32::max),
                    ys.fold(f32::MIN, f32::max),
                ]);
            }
        }
    }
}

/// A PDF text string: UTF-16BE with a byte order mark, or PDFDocEncoding,
/// which for the characters written here is ASCII.
pub fn decode_text_string(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = b[2..]
            .chunks(2)
            .filter(|p| p.len() == 2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        char::decode_utf16(units).filter_map(Result::ok).collect()
    } else {
        b.iter().map(|&c| char::from(c)).collect()
    }
}

/// What the patch did to one PDF.
#[derive(Default, Debug)]
pub struct Patched {
    /// Clusters wrapped: page, the MCID pair, the cluster read twice, the
    /// ActualText given.
    pub clusters: Vec<(usize, i64, i64, String, String)>,
    pub labels: usize,
    /// Labels whose image the patch could not place: count mismatch, or a
    /// span not found exactly once in its stream.
    pub unmatched: usize,
}

/// The name of the invisible layer's font in a page's resources.
const LAYER_FONT: &[u8] = b"ArunaLabel";

fn inverse(m: [f32; 6]) -> [f32; 6] {
    let det = m[0] * m[3] - m[1] * m[2];
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    [a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)]
}

/// The glyph name of a code point by the Adobe Glyph List convention, for a
/// reader that has no ToUnicode and reads names.
fn glyph_name(cp: u32) -> String {
    if cp <= 0xFFFF {
        format!("uni{cp:04X}")
    } else {
        format!("u{cp:X}")
    }
}

/// The font of the invisible layer: Type3, one code per code point, each
/// glyph's procedure `1000 0 d0` and nothing else – it paints nothing and
/// embeds no outline – and a ToUnicode that gives the code point.
fn layer_font(doc: &mut Document, cps: &[(u32, i64)]) -> ObjectId {
    use lopdf::{dictionary, Stream};
    let mut procs = lopdf::Dictionary::new();
    let mut differences = vec![Object::Integer(1)];
    let mut bfchar = String::new();
    for (k, (cp, _)) in cps.iter().enumerate() {
        let name = glyph_name(*cp);
        let proc_id = doc.add_object(Stream::new(dictionary! {}, b"1000 0 d0".to_vec()));
        procs.set(name.as_bytes(), Object::Reference(proc_id));
        differences.push(Object::Name(name.into_bytes()));
        let c = char::from_u32(*cp).unwrap_or('\u{FFFD}');
        let hex: String = c
            .encode_utf16(&mut [0u16; 2])
            .iter()
            .map(|u| format!("{u:04X}"))
            .collect();
        bfchar.push_str(&format!("<{:02X}> <{hex}>\n", k + 1));
    }
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Aruna-Label-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n{} beginbfchar\n{bfchar}endbfchar\nendcmap\nCMapName currentdict /CMapResource defineresource pop\nend\nend\n",
        cps.len()
    );
    let to_unicode = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    let n = cps.len() as i64;
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 0.into(), 0.into()],
        "FontMatrix" => vec![0.001.into(), 0.into(), 0.into(), 0.001.into(), 0.into(), 0.into()],
        "CharProcs" => procs,
        "Encoding" => dictionary! { "Type" => "Encoding", "Differences" => differences },
        "FirstChar" => 1,
        "LastChar" => n,
        "Widths" => cps.iter().map(|(_, w)| Object::Integer(*w)).collect::<Vec<_>>(),
        "ToUnicode" => Object::Reference(to_unicode),
        "Resources" => dictionary! {},
    })
}

/// Puts the font of the invisible layer into a page's resources.
fn add_font_resource(doc: &mut Document, page: ObjectId, font: ObjectId) -> Result<(), PdfError> {
    let resources = doc
        .get_dictionary(page)
        .and_then(|d| d.get(b"Resources"))
        .map_err(broken)?
        .clone();
    let dict = match resources {
        Object::Reference(id) => doc.get_dictionary_mut(id).map_err(broken)?,
        Object::Dictionary(_) => doc
            .get_dictionary_mut(page)
            .and_then(|d| d.get_mut(b"Resources"))
            .and_then(Object::as_dict_mut)
            .map_err(broken)?,
        _ => return Err(broken("page resources of an unknown form")),
    };
    let fonts = match dict.get(b"Font") {
        Ok(Object::Reference(id)) => {
            let id = *id;
            doc.get_dictionary_mut(id).map_err(broken)?
        }
        Ok(Object::Dictionary(_)) => dict
            .get_mut(b"Font")
            .and_then(Object::as_dict_mut)
            .map_err(broken)?,
        _ => {
            dict.set("Font", lopdf::Dictionary::new());
            dict.get_mut(b"Font")
                .and_then(Object::as_dict_mut)
                .map_err(broken)?
        }
    };
    if fonts.has(LAYER_FONT) {
        return Err(broken("the page already names a font ArunaLabel"));
    }
    fonts.set(LAYER_FONT, Object::Reference(font));
    Ok(())
}

/// Variant B of the owner's decision of 27.09.2026. Typst sets every label
/// as an image – the outlines of `U+XXXX`, so no reader extracts the letters
/// – inside a marked-content span with an MCID of its own. Into each such
/// span this writes the code point the label stands for as text in render
/// mode 3, which paints nothing, in a Type3 font whose glyphs are empty,
/// over the image's box: a reader extracts the code point, and only it.
/// `sequence` is the labels in the order the page sets them; the images are
/// matched to it in stream order, and the count has to agree.
/// One label as the page sets it: the code point, the image's height in em
/// of the text around it, and the share of that height below the baseline.
#[derive(Clone, Copy)]
pub struct Label {
    pub cp: u32,
    pub height_em: f32,
    pub depth_share: f32,
    /// The image's width in em of the text around it.
    pub width_em: f32,
}

fn add_invisible_layer(
    doc: &mut Document,
    sequence: &[Label],
    report: &mut Patched,
) -> Result<(), PdfError> {
    report.labels = sequence.len();
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let found = label_images(doc, &pages)?;
    if found.len() != sequence.len() {
        return Err(PdfError::Invariant(Invariant::LabelsUnplaced {
            in_text: sequence.len(),
            in_file: found.len(),
        }));
    }
    let cps = layer_codes(sequence)?;
    let font = layer_font(doc, &cps);
    let code = |cp: u32| cps.iter().position(|c| c.0 == cp).map_or(0, |i| i + 1);
    let mut k = 0;
    for page in pages {
        let here: Vec<(i64, [f32; 6], [f32; 6], Label)> = found
            .iter()
            .filter(|f| f.0 == page)
            .zip(sequence[k..].iter())
            .map(|(f, l)| (f.1, f.2, f.3, *l))
            .collect();
        k += here.len();
        if here.is_empty() {
            continue;
        }
        add_font_resource(doc, page, font)?;
        let stream = only_stream(doc, page)?;
        let mut bytes = stream.get_plain_content().map_err(broken)?;
        for (mcid, at_open, frame, label) in here {
            let needle = format!("/Span<</MCID {mcid}>>BDC");
            let at: Vec<usize> = bytes
                .windows(needle.len())
                .enumerate()
                .filter(|(_, w)| *w == needle.as_bytes())
                .map(|(i, _)| i)
                .collect();
            let [at] = at.as_slice() else {
                report.unmatched += 1;
                continue;
            };
            let layer = layer_text(at_open, frame, &label, code(label.cp));
            let end = at + needle.len();
            bytes.splice(end..end, layer.into_bytes());
        }
        replace_content(stream, bytes)?;
    }
    if report.unmatched > 0 {
        return Err(PdfError::Invariant(Invariant::LabelSpans(report.unmatched)));
    }
    Ok(())
}

/// A label image as found: its page, MCID, the transformation where its
/// span opens and its clip as a frame.
type LabelImage = (ObjectId, i64, [f32; 6], [f32; 6]);

/// The label images of each page, in stream order: a `/Span` with an MCID,
/// a clip and no glyph.
fn label_images(doc: &Document, pages: &[ObjectId]) -> Result<Vec<LabelImage>, PdfError> {
    let mut found = Vec::new();
    for page in pages {
        let scan = scan_page(doc, *page)?;
        for span in &scan.spans {
            if let (Some(mcid), Some(c)) = (span.mcid, span.clip) {
                if span.tag == "Span" && span.glyphs == 0 {
                    let frame = [c[2] - c[0], 0.0, 0.0, c[3] - c[1], c[0], c[1]];
                    found.push((*page, mcid, span.ctm, frame));
                }
            }
        }
    }
    Ok(found)
}

/// One code per code point, as wide as its label, in thousandths of the
/// size: a reader that assembles lines from geometry sees the glyph fill the
/// label's place, as the letters of a text label did.
fn layer_codes(sequence: &[Label]) -> Result<Vec<(u32, i64)>, PdfError> {
    let mut cps: Vec<(u32, i64)> = sequence
        .iter()
        .map(|l| (l.cp, (l.width_em * 1000.0).round() as i64))
        .collect();
    cps.sort_unstable();
    cps.dedup();
    if cps.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(broken("one code point, two label widths"));
    }
    Ok(cps)
}

/// Text of the size of the line, on its baseline, one glyph as wide as the
/// image: a reader that assembles lines from geometry sees one more glyph of
/// the line.
fn layer_text(at_open: [f32; 6], frame: [f32; 6], label: &Label, code: usize) -> String {
    let h = frame[3];
    let size = h / label.height_em;
    let base = [
        1.0,
        0.0,
        0.0,
        1.0,
        frame[4],
        frame[5] + h * label.depth_share,
    ];
    let m = mul(base, inverse(at_open));
    format!(
        "\nq BT 3 Tr /ArunaLabel {size:.4} Tf {:.4} {:.4} {:.4} {:.4} {:.4} {:.4} Tm <{:02X}> Tj ET Q\n",
        m[0], m[1], m[2], m[3], m[4], m[5], code
    )
}

use super::layout::combining;

/// Variant A of the owner's decision of 27.09.2026. Typst 0.15.1 cuts a line
/// into pieces of text by vertical shift (`typst-layout`,
/// `inline/shaping.rs:347–358`), so a combining mark placed with a shift
/// lands in a marked-content span of its own; krilla gives the base glyph an
/// `/ActualText` of the whole cluster and the mark's glyph a ToUnicode of the
/// whole cluster too, and the cluster is read twice (typst/typst #4225).
/// Where a span with an MCID holds such an ActualText and the next MCID span
/// reads exactly that cluster, both are wrapped in one `/Span` whose
/// `/ActualText` is the text of the first: what the source says, once.
fn wrap_clusters(doc: &mut Document, report: &mut Patched) -> Result<(), PdfError> {
    use lopdf::content::Content;
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    for (n, page) in pages {
        let wraps = repeats(&scan_page(doc, page)?, n)?;
        if wraps.is_empty() {
            continue;
        }
        let content = doc.get_page_content(page);
        let mut ops = Content::decode(&content).map_err(broken)?.operations;
        wrap(&mut ops, &wraps, n, report);
        let bytes = Content { operations: ops }.encode().map_err(broken)?;
        replace_content(only_stream(doc, page)?, bytes)?;
    }
    Ok(())
}

/// One cluster read twice: the operations that open the first piece and
/// close the second, the text of the first, the two MCIDs, the cluster.
type Repeat = (usize, usize, String, i64, i64, String);

/// The pairs of neighbouring pieces of text on page `n` where the second
/// reads a cluster the first already reads.
fn repeats(scan: &PageScan, n: u32) -> Result<Vec<Repeat>, PdfError> {
    let mut wraps: Vec<Repeat> = Vec::new();
    // Top-level spans with an MCID, in stream order: the pieces of text.
    let pieces: Vec<&Span> = scan
        .spans
        .iter()
        .filter(|s| s.mcid.is_some() && s.parent.is_none())
        .collect();
    for pair in pieces.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let t = &b.resolved;
        let mut chars = t.chars();
        // A cluster: one character and combining marks after it, or
        // combining marks alone, at least one mark.
        if chars.next().is_none() || !t.chars().any(combining) || !chars.all(combining) {
            continue;
        }
        // The piece before already reads the whole cluster: through its
        // own ActualText or through the ToUnicode of its base glyph.
        if !a.resolved.ends_with(t.as_str()) {
            continue;
        }
        let (Some(ma), Some(mb), Some(close)) = (a.mcid, b.mcid, b.close_op) else {
            continue;
        };
        // Two repeats in a row share a piece: their wrappers would
        // overlap, and the document is refused (owner's decision of
        // 2026-10-02) rather than the build stopped.
        if wraps.last().is_some_and(|w| w.1 >= a.open_op) {
            return Err(PdfError::Clusters { page: n, mcid: ma });
        }
        wraps.push((a.open_op, close, a.resolved.clone(), ma, mb, t.clone()));
    }
    Ok(wraps)
}

/// Wraps each repeat in one `/Span` with the `/ActualText` of its first
/// piece, from the last so that the indices of the earlier ones hold.
fn wrap(ops: &mut Vec<lopdf::content::Operation>, wraps: &[Repeat], n: u32, report: &mut Patched) {
    use lopdf::content::Operation;
    use lopdf::StringFormat;
    for (open, close, text, ma, mb, cluster) in wraps.iter().rev() {
        ops.insert(*close + 1, Operation::new("EMC", vec![]));
        let mut props = lopdf::Dictionary::new();
        let mut utf16 = vec![0xFE, 0xFF];
        for u in text.encode_utf16() {
            utf16.extend(u.to_be_bytes());
        }
        props.set(
            "ActualText",
            Object::String(utf16, StringFormat::Hexadecimal),
        );
        ops.insert(
            *open,
            Operation::new(
                "BDC",
                vec![Object::Name(b"Span".to_vec()), Object::Dictionary(props)],
            ),
        );
        report
            .clusters
            .push((n as usize, *ma, *mb, cluster.clone(), text.clone()));
    }
}

/// The one content stream of a page. Typst writes one per page; a patch that
/// met more would not know which of them its bytes belong to.
fn only_stream(doc: &mut Document, page: ObjectId) -> Result<&mut lopdf::Stream, PdfError> {
    let contents = doc.get_page_contents(page);
    let [id] = contents.as_slice() else {
        return Err(broken(format!(
            "a page with {} content streams",
            contents.len()
        )));
    };
    doc.get_object_mut(*id)
        .and_then(|o| o.as_stream_mut())
        .map_err(broken)
}

/// A page's patched content, written back compressed.
fn replace_content(stream: &mut lopdf::Stream, bytes: Vec<u8>) -> Result<(), PdfError> {
    stream.set_plain_content(bytes);
    stream.compress().map_err(broken)
}

/// Everything done to a PDF after Typst, in one load and one save: the
/// invisible layer of the labels (variant B) and the wrapping of repeated
/// clusters (variant A). A file that needs neither is returned as it came,
/// byte for byte; `clusters` says whether to look for repeats at all, and
/// is known from the source: a combining mark of U+0300–U+036F.
pub fn finish(
    pdf: &[u8],
    labels: &[Label],
    clusters: bool,
) -> Result<(Vec<u8>, Patched), PdfError> {
    let mut report = Patched::default();
    if labels.is_empty() && !clusters {
        return Ok((pdf.to_vec(), report));
    }
    let mut doc = Document::load_mem(pdf).map_err(broken)?;
    if !labels.is_empty() {
        add_invisible_layer(&mut doc, labels, &mut report)?;
    }
    if clusters {
        wrap_clusters(&mut doc, &mut report)?;
    }
    if labels.is_empty() && report.clusters.is_empty() {
        return Ok((pdf.to_vec(), report));
    }
    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(broken)?;
    Ok((out, report))
}

#[cfg(test)]
#[path = "post_tests.rs"]
mod tests;
