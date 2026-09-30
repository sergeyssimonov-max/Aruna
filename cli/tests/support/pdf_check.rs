//! The checks of a built PDF that do not run in every export: the tests and
//! the gate run them (owner's decision of 2026-09-30, question 6). Brought
//! over from the fourth trial by hand – the project's own reader on `lopdf`,
//! the tag tree written out as XML, the checks of one PDF, the negative
//! controls of the structural criterion.
//!
//! Included with `#[path]` by `tests/pdf_module.rs` and by the example
//! `pdf_gate`, so not every item is used by every one of them.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use aruna::pdf::layout::{runs, Page, Piece, Run};
use aruna::pdf::post::{decode_text_string, scan_page, PageScan};
use aruna::pdf::Fonts;
use lopdf::{Dictionary, Document, Object, ObjectId};

/// The page margin of the template, in points: 2 cm.
pub const MARGIN: f32 = 2.0 / 2.54 * 72.0;

// ---------------------------------------------------------------------------
// The reader
// ---------------------------------------------------------------------------

/// Every page of a PDF, walked.
pub fn scan_pdf(pdf: &[u8]) -> Result<Vec<PageScan>, String> {
    let doc = Document::load_mem(pdf).map_err(|e| e.to_string())?;
    doc.get_pages()
        .values()
        .map(|p| scan_page(&doc, *p).map_err(|e| e.to_string()))
        .collect()
}

/// Glyphs and path points outside the type area: `margin` points in from
/// each edge of the MediaBox. Horizontal extents strictly, vertically the
/// baseline. A glyph whose ToUnicode entry is whitespace draws nothing – the
/// spaces Typst trims at a line's end still carry their font width – and is
/// counted apart ([`trimmed_spaces_outside`]).
pub fn outside(scan: &PageScan, margin: f32) -> Vec<String> {
    let eps = 0.01;
    let (left, right) = (margin - eps, scan.width - margin + eps);
    let (bottom, top) = (margin - eps, scan.height - margin + eps);
    let mut out = Vec::new();
    for g in &scan.glyphs {
        if !g.text.is_empty() && g.text.chars().all(char::is_whitespace) {
            continue;
        }
        if g.x0 < left || g.x1 > right || g.y < bottom || g.y > top {
            out.push(format!(
                "glyph {} of {} at x {:.2}–{:.2}, y {:.2}",
                g.code, g.font, g.x0, g.x1, g.y
            ));
        }
    }
    for (x, y) in &scan.path_points {
        if *x < left || *x > right || *y < 0.0 || *y > scan.height {
            out.push(format!("path point at x {x:.2}, y {y:.2}"));
        }
    }
    out
}

/// Whitespace glyphs past the right edge of the type area: the trimmed spaces.
pub fn trimmed_spaces_outside(scan: &PageScan, margin: f32) -> usize {
    let right = scan.width - margin + 0.01;
    scan.glyphs
        .iter()
        .filter(|g| !g.text.is_empty() && g.text.chars().all(char::is_whitespace) && g.x1 > right)
        .count()
}

/// Non-whitespace code points, in order – the comparison of §2, never
/// normalised.
pub fn cps(s: &str) -> Vec<char> {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Where two sequences of code points first part.
pub fn first_difference(a: &[char], b: &[char]) -> Option<(usize, String, String)> {
    let at = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())))?;
    let show = |s: &[char]| {
        s.iter()
            .skip(at)
            .take(6)
            .map(|c| format!("U+{:04X}", u32::from(*c)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    Some((at, show(a), show(b)))
}

/// Removes the one expected occurrence of `needle` from `hay`; how many
/// occurrences there were.
pub fn remove_once(hay: &mut Vec<char>, needle: &[char]) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let found: Vec<usize> = hay
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| i)
        .collect();
    if let Some(at) = found.first() {
        hay.drain(*at..*at + needle.len());
    }
    found.len()
}

/// The text of the page, code point for code point, labels as the code point
/// they stand for: what §2 compares, in logical order.
pub fn expected_text(page: &Page) -> String {
    page.texts().collect::<Vec<_>>().join("\n")
}

/// The checks of one PDF.
#[derive(Default, Debug)]
pub struct Checked {
    pub pages: usize,
    /// Glyphs or path points outside the type area.
    pub outside: Vec<String>,
    pub trimmed_spaces: usize,
    /// Glyph 0 shown, in any font.
    pub notdef: usize,
    /// Occurrences of the credit, page by page.
    pub credit_pages: Vec<usize>,
    /// Whether a subset of UllikummiA is in any page's resources.
    pub ullikummi_embedded: bool,
    /// Spans that carry an `/ActualText`, by the code points they give.
    pub actual_texts: BTreeMap<String, usize>,
    /// The ActualText-aware reader's text in stream order, the credit taken
    /// once from page one, non-whitespace code points.
    pub stream: Vec<char>,
    /// The same without ActualText: what a reader that ignores it gets.
    pub raw: String,
    /// Letters of a label (`U+` followed by hex) the reader extracted.
    pub label_letters: usize,
}

impl Checked {
    /// Whether the credit is where the decision puts it: once, on page one,
    /// in a file that embeds UllikummiA, and nowhere in a file that does not.
    pub fn credit_ok(&self) -> bool {
        let total: usize = self.credit_pages.iter().sum();
        if self.ullikummi_embedded {
            total == 1 && self.credit_pages.first() == Some(&1)
        } else {
            total == 0
        }
    }

    /// The stream's code points and the source's are one multiset: nothing
    /// lost, nothing added, whatever the order the notes are placed in.
    pub fn same_composition(&self, page: &Page) -> bool {
        let mut a = self.stream.clone();
        let mut b = cps(&expected_text(page));
        a.sort_unstable();
        b.sort_unstable();
        a == b
    }
}

pub fn check(pdf: &[u8]) -> Result<Checked, String> {
    let pages = scan_pdf(pdf)?;
    let credit: Vec<char> = cps(aruna::fonts::CREDIT);
    let mut out = Checked {
        pages: pages.len(),
        ..Checked::default()
    };
    for (n, p) in pages.iter().enumerate() {
        for o in outside(p, MARGIN) {
            out.outside.push(format!("page {}: {o}", n + 1));
        }
        out.trimmed_spaces += trimmed_spaces_outside(p, MARGIN);
        out.notdef += p.notdef.values().sum::<usize>();
        let mut page_cps = cps(&p.text);
        let found = remove_once(&mut page_cps, &credit);
        if n > 0 && found > 0 {
            page_cps = cps(&p.text);
        }
        out.credit_pages.push(found);
        out.stream.extend(page_cps);
        out.raw.push_str(&p.raw_text);
        for span in &p.spans {
            if let Some(t) = &span.actual_text {
                let key = t
                    .chars()
                    .map(|c| format!("U+{:04X}", u32::from(c)))
                    .collect::<Vec<_>>()
                    .join(" ");
                *out.actual_texts.entry(key).or_insert(0) += 1;
            }
        }
        if p.fonts.iter().any(|f| f.contains("UllikummiA")) {
            out.ullikummi_embedded = true;
        }
    }
    out.label_letters = out
        .raw
        .match_indices("U+")
        .filter(|(i, _)| {
            out.raw[i + 2..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_hexdigit())
        })
        .count();
    Ok(out)
}

// ---------------------------------------------------------------------------
// The tag tree, reading 1 of the structural criterion
// ---------------------------------------------------------------------------

/// Whitespace runs to one space, trimmed; every other code point as it is.
pub fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text escaped for XML character data and attribute values.
pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

struct Walk<'a> {
    doc: &'a Document,
    text: HashMap<ObjectId, HashMap<i64, String>>,
    out: String,
    seen: HashMap<(ObjectId, i64), usize>,
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    match o {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(o),
        _ => o,
    }
}

fn mcr(d: &Dictionary, page: Option<ObjectId>) -> (Option<ObjectId>, i64) {
    let pg = d
        .get(b"Pg")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .or(page);
    (pg, d.get(b"MCID").and_then(|o| o.as_i64()).unwrap_or(-1))
}

impl Walk<'_> {
    fn mcid(&mut self, page: Option<ObjectId>, mcid: i64, buf: &mut String) {
        let Some(page) = page else {
            buf.push('\u{FFFD}');
            return;
        };
        *self.seen.entry((page, mcid)).or_insert(0) += 1;
        if let Some(t) = self.text.get(&page).and_then(|m| m.get(&mcid)) {
            buf.push_str(t);
        }
    }

    fn element(&mut self, dict: &Dictionary, page: Option<ObjectId>) {
        let role = dict
            .get(b"S")
            .and_then(|o| o.as_name())
            .or_else(|_| dict.get(b"Type").and_then(|o| o.as_name()))
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_else(|_| "Unknown".into());
        let page = dict
            .get(b"Pg")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .or(page);
        let _ = write!(self.out, "<{role}");
        for key in [b"Alt".as_slice(), b"ActualText".as_slice()] {
            if let Ok(v) = dict.get(key).and_then(|o| o.as_str()) {
                let _ = write!(
                    self.out,
                    " {}=\"{}\"",
                    String::from_utf8_lossy(key),
                    escape(&decode_text_string(v))
                );
            }
        }
        let attrs: Vec<Object> = match dict.get(b"A").map(|o| deref(self.doc, o)) {
            Ok(Object::Array(a)) => a.clone(),
            Ok(o) => vec![o.clone()],
            Err(_) => Vec::new(),
        };
        for a in &attrs {
            let Ok(a) = deref(self.doc, a).as_dict() else {
                continue;
            };
            let owner = a
                .get(b"O")
                .and_then(|o| o.as_name())
                .map(|n| String::from_utf8_lossy(n).into_owned())
                .unwrap_or_default();
            for (k, v) in a.iter() {
                if k.as_slice() == b"O" {
                    continue;
                }
                let v = match deref(self.doc, v) {
                    Object::Name(n) => String::from_utf8_lossy(n).into_owned(),
                    Object::Integer(i) => i.to_string(),
                    Object::Real(r) => r.to_string(),
                    Object::String(s, _) => decode_text_string(s),
                    other => format!("{other:?}"),
                };
                let _ = write!(
                    self.out,
                    " {owner}.{}=\"{}\"",
                    String::from_utf8_lossy(k),
                    escape(&v)
                );
            }
        }
        self.out.push('>');
        let kids: Vec<Object> = match dict.get(b"K").map(|o| deref(self.doc, o)) {
            Ok(Object::Array(a)) => a.clone(),
            Ok(o) => vec![o.clone()],
            Err(_) => Vec::new(),
        };
        let mut buf = String::new();
        for kid in kids {
            match &kid {
                Object::Integer(m) => self.mcid(page, *m, &mut buf),
                Object::Reference(id) => {
                    let Ok(d) = self.doc.get_dictionary(*id) else {
                        continue;
                    };
                    let kind = d.get(b"Type").and_then(|o| o.as_name()).ok();
                    if kind == Some(b"MCR".as_slice()) {
                        let (pg, m) = mcr(d, page);
                        self.mcid(pg, m, &mut buf);
                    } else if kind == Some(b"OBJR".as_slice()) {
                        continue;
                    } else {
                        self.flush(&mut buf);
                        let d = d.clone();
                        self.element(&d, page);
                    }
                }
                Object::Dictionary(d) => {
                    if d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"MCR".as_slice()) {
                        let (pg, m) = mcr(d, page);
                        self.mcid(pg, m, &mut buf);
                    } else {
                        self.flush(&mut buf);
                        let d = d.clone();
                        self.element(&d, page);
                    }
                }
                _ => {}
            }
        }
        self.flush(&mut buf);
        let _ = write!(self.out, "</{role}>");
    }

    fn flush(&mut self, buf: &mut String) {
        let t = squeeze(buf);
        if !t.is_empty() {
            self.out.push_str(&escape(&t));
        }
        buf.clear();
    }
}

/// What a tree export found besides the tree.
#[derive(Default, Debug)]
pub struct Exported {
    pub xml: String,
    /// Marked content with an MCID that no structure element names.
    pub orphans: usize,
    /// MCIDs named by more than one structure element.
    pub twice: usize,
}

/// The tag tree of a PDF as XML: the roots of `StructTreeRoot /K` under one
/// `<Tree>` element.
pub fn export_tree(pdf: &[u8]) -> Result<Exported, String> {
    let doc = Document::load_mem(pdf).map_err(|e| e.to_string())?;
    let root = doc
        .catalog()
        .and_then(|c| c.get(b"StructTreeRoot"))
        .map(|o| deref(&doc, o).clone())
        .map_err(|_| "no StructTreeRoot: the PDF is not tagged".to_string())?;
    let root = root.as_dict().map_err(|e| e.to_string())?.clone();
    let mut text = HashMap::new();
    let mut all: Vec<(ObjectId, i64)> = Vec::new();
    for page in doc.get_pages().values() {
        let scan = scan_page(&doc, *page).map_err(|e| e.to_string())?;
        let mut by = HashMap::new();
        for s in &scan.spans {
            if let Some(m) = s.mcid {
                by.insert(m, s.resolved.clone());
                all.push((*page, m));
            }
        }
        text.insert(*page, by);
    }
    let mut walk = Walk {
        doc: &doc,
        text,
        out: String::from("<Tree>"),
        seen: HashMap::new(),
    };
    walk.element(&root, None);
    walk.out.push_str("</Tree>");
    let orphans = all.iter().filter(|k| !walk.seen.contains_key(k)).count();
    let twice = walk.seen.values().filter(|n| **n > 1).count();
    Ok(Exported {
        xml: walk.out,
        orphans,
        twice,
    })
}

fn tokens(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let end = if rest.starts_with('<') {
            rest.find('>').map_or(rest.len(), |i| i + 1)
        } else {
            rest.find('<').unwrap_or(rest.len())
        };
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    out
}

/// Where two trees first part: the path of elements down to the place, and
/// what each side has there. `None` when the two are the same string.
pub fn tree_difference(expected: &str, actual: &str) -> Option<(String, String, String)> {
    if expected == actual {
        return None;
    }
    let (a, b) = (tokens(expected), tokens(actual));
    let mut path: Vec<(String, HashMap<String, usize>)> = vec![(String::new(), HashMap::new())];
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i), b.get(i));
        if x != y {
            let p: String = path.iter().skip(1).map(|(n, _)| format!("/{n}")).collect();
            let show = |t: Option<&String>| {
                t.map_or("(end)".to_string(), |t| t.chars().take(160).collect())
            };
            return Some((if p.is_empty() { "/".into() } else { p }, show(x), show(y)));
        }
        let Some(t) = x else { break };
        if t.starts_with("</") {
            path.pop();
        } else if let Some(open) = t.strip_prefix('<') {
            let name: String = open
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if let Some((_, parent)) = path.last_mut() {
                let n = parent.entry(name.clone()).or_insert(0);
                *n += 1;
                let label = format!("{name}[{}]", *n);
                path.push((label, HashMap::new()));
            }
        }
    }
    Some(("/".into(), "(same tokens)".into(), "(same tokens)".into()))
}

/// The text of a tree: every text node in order, the markup dropped – what
/// the tree says, in logical order.
pub fn tree_text(xml: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in xml.chars() {
        match c {
            '<' => {
                inside = true;
                out.push(' ');
            }
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// The tag tree the document model gives through the map of `template.typ`
/// (see the table at its head).
pub fn expected_tree(fonts: &Fonts, page: &Page, credit: bool) -> String {
    fn text(out: &mut String, buf: &mut String) {
        out.push_str(&escape(&squeeze(buf)));
        buf.clear();
    }
    fn runs_into(fonts: &Fonts, s: &str, out: &mut String, buf: &mut String) {
        for r in runs(fonts, s) {
            match r {
                Run::Text(t) | Run::Series(t) => buf.push_str(&t),
                Run::Label(cp) => {
                    text(out, buf);
                    out.push_str(&format!(
                        "<Figure Layout.Placement=\"Block\">{}</Figure>",
                        escape(&char::from_u32(cp).unwrap_or('\u{FFFD}').to_string())
                    ));
                }
            }
        }
    }
    let mut out = String::from("<Tree><StructTreeRoot><Document>");
    let mut buf = String::new();
    if credit {
        out.push_str(&format!("<P>{}</P>", escape(aruna::fonts::CREDIT)));
    }
    for (tag, s) in [("H1", &page.head), ("H2", &page.name)] {
        out.push_str(&format!("<{tag}>"));
        runs_into(fonts, s, &mut out, &mut buf);
        text(&mut out, &mut buf);
        out.push_str(&format!("</{tag}>"));
    }
    for section in &page.sections {
        out.push_str("<Div><Div>");
        for line in section {
            out.push_str("<P>");
            for p in line {
                match p {
                    Piece::Text(t) => runs_into(fonts, t, &mut out, &mut buf),
                    Piece::Note(c) => {
                        text(&mut out, &mut buf);
                        out.push_str("<Lbl><Link></Link></Lbl><Note><Lbl><Link></Link></Lbl>");
                        runs_into(fonts, c, &mut out, &mut buf);
                        text(&mut out, &mut buf);
                        out.push_str("</Note>");
                    }
                    Piece::Tab => {}
                }
            }
            text(&mut out, &mut buf);
            out.push_str("</P>");
        }
        out.push_str("</Div></Div>");
    }
    out.push_str("</Document></StructTreeRoot></Tree>");
    out
}

// ---------------------------------------------------------------------------
// Negative controls of the structural criterion
// ---------------------------------------------------------------------------

/// (a) two neighbouring lines swapped, in the first section with two.
pub fn swap_lines(page: &Page) -> Option<Page> {
    let mut p = page.clone();
    let s = p.sections.iter_mut().find(|s| s.len() > 1)?;
    s.swap(0, 1);
    (p != *page).then_some(p)
}

/// (b) the first note left out.
pub fn drop_note(page: &Page) -> Option<Page> {
    let mut p = page.clone();
    for line in p.sections.iter_mut().flatten() {
        if let Some(i) = line.iter().position(|x| matches!(x, Piece::Note(_))) {
            line.remove(i);
            return Some(p);
        }
    }
    None
}

/// (c) the first line set twice.
pub fn repeat_line(page: &Page) -> Option<Page> {
    let mut p = page.clone();
    let s = p.sections.first_mut()?;
    let line = s.first()?.clone();
    s.insert(0, line);
    Some(p)
}

/// (d) the first label's invisible text shows its code twice.
pub fn spoil_label(pdf: &[u8]) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(pdf).map_err(|e| e.to_string())?;
    for page in doc.get_pages().values().copied().collect::<Vec<_>>() {
        let contents = doc.get_page_contents(page);
        let [id] = contents.as_slice() else { continue };
        let stream = doc
            .get_object_mut(*id)
            .and_then(|o| o.as_stream_mut())
            .map_err(|e| e.to_string())?;
        let mut bytes = stream.get_plain_content().map_err(|e| e.to_string())?;
        let find = |hay: &[u8], needle: &[u8], from: usize| {
            hay[from..]
                .windows(needle.len())
                .position(|w| w == needle)
                .map(|i| from + i)
        };
        let Some(at) = find(&bytes, b"/ArunaLabel", 0) else {
            continue;
        };
        let close = find(&bytes, b">", at).ok_or("no string")?;
        let open = bytes[..close]
            .iter()
            .rposition(|b| *b == b'<')
            .ok_or("no string")?;
        let code = bytes[open + 1..close].to_vec();
        bytes.splice(close..close, code);
        stream.set_plain_content(bytes);
        stream.compress().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        doc.save_to(&mut out).map_err(|e| e.to_string())?;
        return Ok(out);
    }
    Err("no label".into())
}
