//! Markup as one stream of events, for the body of a document and for the
//! markup the corpus carries as text: a note's `c` and a line's `cu`
//! (`XML-CONTRACT.md` §4, "What a page shows"). The rules of display read the
//! stream and nothing else, so a tag shows the same in the body and in a note.
//!
//! The text of an attribute is read by the parser of the documents, as a
//! fragment inside a root that binds the two prefixes the corpus writes. A
//! fragment that parser will not read – crossed tags, a tag cut short – goes
//! to a lenient reader: a tag is `<`, a name, anything but `<` up to `>`;
//! everything else is text. Neither ever drops text, and neither fails.

use std::borrow::Cow;

use crate::document::{Document, Element, Kind};

/// The names of the markup the corpus writes in its notes and `cu`, which it
/// also writes between `〈` and `〉` (U+2329, U+232A): 84 in the notes and 4
/// in `cu` as measured on TLHdig Beta 0.3, and 4 more the owner added on
/// 2026-10-27 (`XML-CONTRACT.md` §4, "What a page shows"). A name between those brackets that is
/// not here is text: an emendation, `ku-〈e〉-da`.
pub const VOCABULARY: [&str; 90] = [
    "AO-LIT",
    "AO:LIT",
    "AO:Lit",
    "AO:SIGN",
    "Akk",
    "AkkGRAM",
    "Hit",
    "Hittite",
    "LIT",
    "Numeral",
    "P_f_",
    "P_f_AO_3a_Exemplar",
    "P_f_AO_3a_Fußnote",
    "P_f_AO_3a_Textline-Akk",
    "P_f_AO_3a_Textline-Hit",
    "P_f_Endnote",
    "P_f_Footnote",
    "P_f_Footnote_20_Text",
    "P_f_Standard",
    "P_f_Text_20_body",
    "P_f_sdfootnote",
    "Pal",
    "SP___AO_3a_--italic",
    "SP_f_AO_3a_--index",
    "SP_f_AO_3a_--italic",
    "SP_f_AO_3a_--subscr",
    "SP_f_AO_3a_--superscr",
    "SP_f_AO_3a_-Hittite",
    "SP_f_AO_3a_-LIT",
    "SP_f_AO_3a_-LineNrExpl",
    "SP_f_AO_3a_-Text-Gap",
    "SP_f_AO_3a_AkkGRAM",
    "SP_f_AO_3a_Akkadian",
    "SP_f_AO_3a_Akkgram",
    "SP_f_AO_3a_Det",
    "SP_f_AO_3a_Determ",
    "SP_f_AO_3a_DocSigl",
    "SP_f_AO_3a_Hattian",
    "SP_f_AO_3a_Hittite",
    "SP_f_AO_3a_Hurrian",
    "SP_f_AO_3a_InvNr",
    "SP_f_AO_3a_LIT",
    "SP_f_AO_3a_Luwian",
    "SP_f_AO_3a_MaterLect",
    "SP_f_AO_3a_Numeral",
    "SP_f_AO_3a_Palaic",
    "SP_f_AO_3a_SIGN",
    "SP_f_AO_3a_SumGRAM",
    "SP_f_AO_3a_Sumerian",
    "SP_f_AO_3a_Sumgram",
    "SP_f_AO_3a_TabSep",
    "SP_f_AO_3a_TxtPubl",
    "SP_f_AO_3a_corr",
    "SP_f_AO_3a_editor",
    "SP_f_AO_3a_italic",
    "SP_f_AO_3a_numeral",
    "SP_f_AO_3a_sGr",
    "SP_f_AO_3a_subscr",
    "SP_f_AO_3a_sumGRAM",
    "SP_f_AO_3a_Ḫittite",
    "SP_f_AOxx_3a_Notizen",
    "SP_f_Default_20_Paragraph_20_Font",
    "SP_f_Endnote_20_Symbol",
    "SP_f_Footnote_20_Symbol",
    "Sum",
    "aGr",
    "c",
    "corr",
    "d",
    "del_fin",
    "del_in",
    "hit",
    "laes_fin",
    "laes_in",
    "lit",
    "materlect",
    "num",
    "ras_X",
    "ras_fin",
    "ras_in",
    "sG",
    "sGr",
    "space",
    "subscr",
    "text:a",
    "text:change",
    "text:tab",
    "w",
    "wsep",
    "wsepc=\"\u{12039}\"",
];

/// A start tag: the name as written, the attributes by local name, and the
/// text of an element whose one child is a text node.
#[derive(Clone, Debug, PartialEq)]
pub struct Tag<'a> {
    pub prefix: Option<Cow<'a, str>>,
    pub local: Cow<'a, str>,
    pub attributes: Vec<(Cow<'a, str>, Cow<'a, str>)>,
    pub sole_text: Option<Cow<'a, str>>,
}

impl Tag<'_> {
    /// The value of the attribute of this local name.
    pub fn attribute(&self, local: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == local)
            .map(|(_, v)| v.as_ref())
    }

    fn into_owned(self) -> Tag<'static> {
        Tag {
            prefix: self.prefix.map(|p| Cow::Owned(p.into_owned())),
            local: Cow::Owned(self.local.into_owned()),
            attributes: self
                .attributes
                .into_iter()
                .map(|(k, v)| (Cow::Owned(k.into_owned()), Cow::Owned(v.into_owned())))
                .collect(),
            sole_text: self.sole_text.map(|t| Cow::Owned(t.into_owned())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event<'a> {
    Start(Tag<'a>),
    /// The end of an element, by its local name.
    End(Cow<'a, str>),
    Text(Cow<'a, str>),
}

impl Event<'_> {
    fn into_owned(self) -> Event<'static> {
        match self {
            Event::Start(t) => Event::Start(t.into_owned()),
            Event::End(n) => Event::End(Cow::Owned(n.into_owned())),
            Event::Text(t) => Event::Text(Cow::Owned(t.into_owned())),
        }
    }
}

fn tag<'d>(e: &'d Element<'_>, sole_text: Option<&'d str>) -> Tag<'d> {
    Tag {
        prefix: e.name.prefix.as_deref().map(Cow::Borrowed),
        local: Cow::Borrowed(&e.name.local),
        attributes: e
            .attributes
            .iter()
            .map(|a| (Cow::Borrowed(&*a.name.local), Cow::Borrowed(&*a.value)))
            .collect(),
        sole_text: sole_text.map(Cow::Borrowed),
    }
}

/// The events of a document, in document order. Comments and processing
/// instructions are not shown (owner's decision of 2026-09-30) and give none.
pub fn events<'d>(doc: &'d Document<'_>) -> Vec<Event<'d>> {
    let nodes = doc.nodes();
    let mut out = Vec::with_capacity(nodes.len() * 2);
    // Open elements: where each subtree ends, and the element.
    let mut open: Vec<(usize, &'d Element<'_>)> = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        while let Some((_, e)) = open.pop_if(|(end, _)| *end <= i) {
            out.push(Event::End(Cow::Borrowed(&*e.name.local)));
        }
        match &node.kind {
            Kind::Element(e) => {
                let sole_text = match nodes.get(i + 1) {
                    Some(next) if node.end == i + 2 => match &next.kind {
                        Kind::Text(t) => Some(&**t),
                        _ => None,
                    },
                    _ => None,
                };
                out.push(Event::Start(tag(e, sole_text)));
                open.push((node.end, e));
            }
            Kind::Text(t) => out.push(Event::Text(Cow::Borrowed(t))),
            Kind::Comment(_) | Kind::Instruction { .. } => {}
        }
    }
    while let Some((_, e)) = open.pop() {
        out.push(Event::End(Cow::Borrowed(&*e.name.local)));
    }
    out
}

/// The root a fragment is read inside: it binds `AO:` and `text:`.
const OPEN: &str = "<r xmlns:AO=\"http://hethiter.net/ns/AO/1.0\" \
                    xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\">";

/// The events of markup carried as text (a note's `c`, a line's `cu`), its
/// entities already resolved once by the model: the vocabulary's names
/// between `〈` and `〉` become tags, the rest is read as markup.
pub fn fragment(s: &str) -> Vec<Event<'static>> {
    if !s.contains(['<', '&', '\u{2329}']) {
        return vec![Event::Text(Cow::Owned(s.to_string()))];
    }
    let s = angle_tags(s);
    let wrapped = format!("{OPEN}{s}</r>");
    match Document::read(wrapped.as_bytes()) {
        Ok(doc) => {
            let mut ev = events(&doc);
            // The root's own start and end.
            ev.pop();
            ev.into_iter()
                .skip(1)
                .map(|e| match e.into_owned() {
                    // The parser takes a name with stray characters before it
                    // as it is; read it as the lenient reader does.
                    Event::Start(mut t) => {
                        for (k, _) in &mut t.attributes {
                            if name_at_end(k).len() != k.len() {
                                *k = Cow::Owned(name_at_end(k).to_string());
                            }
                        }
                        Event::Start(t)
                    }
                    e => e,
                })
                .collect()
        }
        Err(_) => lenient(&s),
    }
}

/// `〈name〉`, `〈/name〉` and `〈name/〉` (with anything after the name) as
/// tags, when the name is the vocabulary's.
pub fn angle_tags(s: &str) -> Cow<'_, str> {
    if !s.contains('\u{2329}') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('\u{2329}') {
        out.push_str(&rest[..at]);
        let after = &rest[at + '\u{2329}'.len_utf8()..];
        let inner_end = after.find(['\u{2329}', '\u{232A}']);
        match inner_end {
            Some(end)
                if after[end..].starts_with('\u{232A}') && is_vocabulary_tag(&after[..end]) =>
            {
                out.push('<');
                out.push_str(&after[..end]);
                out.push('>');
                rest = &after[end + '\u{232A}'.len_utf8()..];
            }
            _ => {
                out.push('\u{2329}');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// Whether what stands between the brackets is a tag of the vocabulary:
/// `/`?, then a name of it up to whitespace or `/`, then anything.
fn is_vocabulary_tag(inner: &str) -> bool {
    let inner = inner.strip_prefix('/').unwrap_or(inner);
    let name_end = inner
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(inner.len());
    VOCABULARY.contains(&&inner[..name_end])
}

/// The lenient reader: `<`, `/`?, a name (a letter or `_`, then anything but
/// whitespace, `/`, `<`, `>`, `=`, `"`), anything but `<` up to `>` is a tag;
/// `/` before the `>` closes it at once. Everything else is text, the five
/// entities and character references resolved.
pub fn lenient(s: &str) -> Vec<Event<'static>> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = s;
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match lenient_tag(after) {
            Some((len, event, closed)) => {
                if !text.is_empty() {
                    out.push(Event::Text(Cow::Owned(unescape(&text))));
                    text.clear();
                }
                let name = match &event {
                    Event::Start(t) => Some(t.local.clone()),
                    _ => None,
                };
                out.push(event);
                if let (true, Some(name)) = (closed, name) {
                    out.push(Event::End(name));
                }
                rest = &after[len..];
            }
            None => {
                text.push('<');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Event::Text(Cow::Owned(unescape(&text))));
    }
    out
}

/// A tag at the head of `s` (after its `<`): its length up to and with the
/// `>`, its event, and whether it closes itself.
fn lenient_tag(s: &str) -> Option<(usize, Event<'static>, bool)> {
    let close = s.starts_with('/');
    let body = if close { &s[1..] } else { s };
    if !body.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    let end = body.find(['<', '>'])?;
    if !body[end..].starts_with('>') {
        return None;
    }
    let inner = &body[..end];
    // The name runs to whitespace or `/`, as the parser of the documents
    // reads it: `<wsepc="𒀹"/>` is named `wsepc="𒀹"` by both.
    let name_end = inner
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(inner.len());
    let qualified = &inner[..name_end];
    let (prefix, local) = match qualified.split_once(':') {
        Some((p, l)) => (Some(Cow::Owned(p.to_string())), l),
        None => (None, qualified),
    };
    let closed = inner.ends_with('/');
    let len = usize::from(close) + end + 1;
    let event = if close {
        Event::End(Cow::Owned(local.to_string()))
    } else {
        Event::Start(Tag {
            prefix,
            local: Cow::Owned(local.to_string()),
            attributes: lenient_attributes(&inner[name_end..]),
            sole_text: None,
        })
    };
    Some((len, event, closed && !close))
}

/// A character of a name: a letter, a digit, `_`, `-`, `.` or `:`.
pub fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | ':' | '.' | '-')
}

/// The run of name characters at the end of `s`: the name of an attribute
/// written with something before it – `<wsep !!c=" 𒑱 !!"/>`
/// (`CTH 670/KP24_467C_PL2_SF65`) has a `c`.
fn name_at_end(s: &str) -> &str {
    let start = s
        .char_indices()
        .rev()
        .find(|(_, c)| !is_name_char(*c))
        .map_or(0, |(i, c)| i + c.len_utf8());
    &s[start..]
}

/// `name="value"` pairs, by local name, the value's entities resolved.
fn lenient_attributes(mut s: &str) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    let mut out = Vec::new();
    while let Some(eq) = s.find('=') {
        let name = name_at_end(s[..eq].trim_end());
        let after = s[eq + 1..].trim_start();
        let Some(value) = after.strip_prefix('"') else {
            break;
        };
        let Some(close) = value.find('"') else {
            break;
        };
        let local = name.rsplit(':').next().unwrap_or(name);
        if !local.is_empty() {
            out.push((
                Cow::Owned(local.to_string()),
                Cow::Owned(unescape(&value[..close])),
            ));
        }
        s = &value[close + 1..];
    }
    out
}

/// The five predefined entities and character references, resolved; any
/// other `&` is text.
fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let resolved = after.find(';').and_then(|semi| {
            let name = &after[..semi];
            let c = match name {
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, semi))
        });
        match resolved {
            Some((c, semi)) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
