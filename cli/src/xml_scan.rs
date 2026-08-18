//! Byte scanners for TLHdig / AOxml header heuristics.
//!
//! The parser never builds a DOM. It only needs a handful of tags and
//! attributes from the first few KiB of each document. Candidate positions
//! are located with `memchr`; everything after that is a plain scalar loop.
//!
//! Design priorities (in order):
//! 1. Correctness and total behaviour (malformed input → None / empty, never panic)
//! 2. Readability
//! 3. Acceptable performance on short fragments
//!
//! Micro-optimisations without measurements are intentionally avoided.

use memchr::{memchr, memchr2, memmem};

/// ASCII case-fold for A–Z only.
fn ascii_lower(b: u8) -> u8 {
    b | 0x20
}

/// True when `b` is ASCII alphabetic.
fn is_ascii_alpha(b: u8) -> bool {
    ascii_lower(b).wrapping_sub(b'a') < 26
}

/// Characters allowed inside an XML Name (local part or prefix).
///
/// `:` is included, so a scanned name still carries its prefix — use
/// [`local_part`] to drop it.
fn is_name_char(b: u8) -> bool {
    is_ascii_alpha(b) || b.is_ascii_digit() || matches!(b, b':' | b'-' | b'_' | b'.')
}

/// Local part of a possibly prefixed XML Name (`AO:docID` → `docID`).
fn local_part(name: &[u8]) -> &[u8] {
    // A plain scan, not `memchr`: a name is a dozen bytes and this runs once
    // per tag, where setting up the vectorised search costs more than it saves.
    match name.iter().position(|&b| b == b':') {
        Some(colon) => &name[colon + 1..],
        None => name,
    }
}

/// Read the element name beginning at `from`: its local part, and the byte
/// after the name.
///
/// Every scanner below starts a tag the same way — take name characters, drop
/// the namespace prefix — and the three copies of that loop were three chances
/// to disagree about which bytes belong to a name.
fn element_name(hay: &[u8], from: usize) -> (&[u8], usize) {
    let mut end = from;
    while end < hay.len() && is_name_char(hay[end]) {
        end += 1;
    }
    (local_part(&hay[from..end]), end)
}

/// Case-insensitive equality of two equal-length byte slices (ASCII).
pub fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(&x, &y)| ascii_lower(x) == ascii_lower(y))
}

/// Case-insensitive search for `needle` in `hay`.
///
/// Uses `memchr2` for the first byte (upper + lower) then verifies.
pub fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }

    let first = needle[0];
    let first_lo = ascii_lower(first);
    let first_up = first_lo.wrapping_sub(b'a').wrapping_add(b'A');
    let use_pair = first_lo != first_up && is_ascii_alpha(first);

    let mut pos = 0;
    while pos + needle.len() <= hay.len() {
        let rel = if use_pair {
            memchr2(first_lo, first_up, &hay[pos..])?
        } else {
            memchr(first, &hay[pos..])?
        };
        let i = pos + rel;
        if i + needle.len() > hay.len() {
            return None;
        }
        if eq_ci(&hay[i..i + needle.len()], needle) {
            return Some(i);
        }
        pos = i + 1;
    }
    None
}

/// Exact (case-sensitive) substring search.
pub fn find_exact(hay: &[u8], needle: &[u8]) -> Option<usize> {
    memmem::find(hay, needle)
}

/// Byte just past the comment, CDATA section, declaration or processing
/// instruction opening at `i`, where `hay[i]` is `<` and the next byte is `!`
/// or `?`.
///
/// Stepping a single byte past such a `<` is not enough: the scanners then look
/// for the next `<`, and a comment may well contain one. A commented-out
/// `<!-- <uebern editor="…"/> -->` would be read as a live element and outrank
/// the real one, because the header heuristics take the first match they find.
///
/// An unterminated construct swallows the rest of the input — everything after
/// an unclosed `<!--` is comment, so there is nothing left worth scanning.
fn skip_non_element(hay: &[u8], i: usize) -> usize {
    let rest = &hay[i..];
    let past = |pat: &[u8]| match memmem::find(rest, pat) {
        Some(p) => i + p + pat.len(),
        None => hay.len(),
    };
    if rest.starts_with(b"<!--") {
        return past(b"-->");
    }
    if rest.starts_with(b"<![CDATA[") {
        return past(b"]]>");
    }
    if rest.starts_with(b"<?") {
        return past(b"?>");
    }
    // `<!DOCTYPE …>` and other declarations
    match memchr(b'>', rest) {
        Some(p) => i + p + 1,
        None => hay.len(),
    }
}

/// One element boundary: `<name …>` or `</name>`.
///
/// Offsets are into the document the [`Tags`] walker was given, so a caller can
/// slice text between two of them.
enum Tag<'a> {
    Start(StartTag<'a>),
    End(EndTag<'a>),
}

/// A `</name>`, with the name left unread until asked for.
///
/// Lazy for the same reason as [`StartTag`], and it matters more: a manuscript
/// body is mostly end tags, and only `find_close_tag` ever wants their names.
/// Reading every one of them cost around 50 ms of the parse stage.
struct EndTag<'a> {
    /// Offset of the `<`.
    at: usize,
    hay: &'a [u8],
}

impl<'a> EndTag<'a> {
    fn name(&self) -> &'a [u8] {
        element_name(self.hay, self.at + 2).0
    }
}

/// A `<name …>`, with everything past the name left unread until asked for.
///
/// Laziness is the point. [`find_open_tag`] walks every start tag in a document
/// and wants the attributes of at most one of them; searching for the `>` of
/// each tag on the way cost 47 % of the parse stage when this was measured.
struct StartTag<'a> {
    /// Offset of the `<`.
    at: usize,
    /// Local part of the name — the namespace prefix is already dropped.
    name: &'a [u8],
    hay: &'a [u8],
    /// Offset just past the name, where the attributes begin.
    after_name: usize,
}

impl<'a> StartTag<'a> {
    /// The rest of the tag: its attributes, and where the element's content
    /// begins. `None` when the document holds no `>` after the name.
    ///
    /// One search for both, because every caller that wants either wants both,
    /// and the search is the expensive part.
    #[inline]
    fn rest(&self) -> Option<(&'a [u8], usize)> {
        let gt = self.after_name + memchr(b'>', self.hay.get(self.after_name..)?)?;
        let attrs = trim_ascii_start(&self.hay[self.after_name..gt]);
        Some((attrs, gt + 1))
    }
}

/// The element boundaries of a document, in order.
///
/// Everything that is markup but not an element — comments, CDATA sections,
/// declarations, processing instructions — is stepped over rather than
/// reported, because the header heuristics take the first match they find and
/// a commented-out `<uebern editor="…"/>` would otherwise outrank the live one.
///
/// This walk was written out three times, in `find_open_tag`, `find_close_tag`
/// and `for_each_start_tag`, and the three had drifted: two of them resumed one
/// byte past the `<` while the third resumed past the `>`, so the same
/// malformed document could be read differently depending on which function
/// looked at it. One walker, one answer.
struct Tags<'a> {
    hay: &'a [u8],
    pos: usize,
}

fn tags(hay: &[u8]) -> Tags<'_> {
    Tags { hay, pos: 0 }
}

impl Tags<'_> {
    /// Carry on from `pos`, skipping whatever lies before it.
    ///
    /// For a caller that has already found where the current tag ends: the walk
    /// itself will not look for a `>`, since most callers never need one.
    fn resume_at(&mut self, pos: usize) {
        self.pos = pos.max(self.pos).min(self.hay.len());
    }
}

fn tags_from(hay: &[u8], from: usize) -> Tags<'_> {
    Tags {
        hay,
        pos: from.min(hay.len()),
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = Tag<'a>;

    #[inline]
    fn next(&mut self) -> Option<Tag<'a>> {
        loop {
            let at = self.pos + memchr(b'<', self.hay.get(self.pos..)?)?;
            let after = *self.hay.get(at + 1)?;

            if after == b'!' || after == b'?' {
                self.pos = skip_non_element(self.hay, at);
                continue;
            }

            if after == b'/' {
                self.pos = at + 2;
                return Some(Tag::End(EndTag { at, hay: self.hay }));
            }

            let (name, after_name) = element_name(self.hay, at + 1);
            // Resume just past the `<` rather than past the `>`: finding the
            // `>` is what this walk deliberately does not pay for.
            self.pos = at + 1;
            return Some(Tag::Start(StartTag {
                at,
                name,
                hay: self.hay,
                after_name,
            }));
        }
    }
}

fn trim_ascii_start(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    &bytes[start..]
}

/// Find the opening tag `<…local…>` (with optional namespace prefix).
/// Returns `(start_of_tag, end_of_opening_tag)` so the caller can slice attributes or text.
pub fn find_open_tag(hay: &[u8], local: &[u8]) -> Option<(usize, usize)> {
    tags(hay).find_map(|tag| match tag {
        Tag::Start(start) if eq_ci(start.name, local) => {
            let (_, content) = start.rest()?;
            Some((start.at, content))
        }
        _ => None,
    })
}

/// The first opening tag for each of `names`, in one walk.
///
/// [`find_open_tag`] walks every tag in the haystack, so asking it for three
/// names walks it three times — and the extractor that wanted three names was
/// doing exactly that, over both windows, for every document in the corpus.
/// This fills `found[i]` with the first tag whose local name matches
/// `names[i]`, and stops as soon as every name has an answer.
///
/// Each name keeps its own first match rather than the earliest match of any
/// name: a caller that prefers one spelling over another needs the first
/// element of *that* spelling, not whichever came first in the document.
pub fn find_open_tags(hay: &[u8], names: &[&[u8]], found: &mut [Option<(usize, usize)>]) {
    debug_assert_eq!(names.len(), found.len());
    let mut remaining = names.len();

    for tag in tags(hay) {
        let Tag::Start(start) = tag else { continue };
        for (i, name) in names.iter().enumerate() {
            if found[i].is_some() || !eq_ci(start.name, name) {
                continue;
            }
            let Some((_, content)) = start.rest() else {
                continue;
            };
            found[i] = Some((start.at, content));
            remaining -= 1;
        }
        if remaining == 0 {
            return;
        }
    }
}

/// Find the matching closing tag `</…local…>` starting search from `from`.
pub fn find_close_tag(hay: &[u8], from: usize, local: &[u8]) -> Option<usize> {
    tags_from(hay, from).find_map(|tag| match tag {
        Tag::End(end) if eq_ci(end.name(), local) => Some(end.at),
        _ => None,
    })
}

/// Convenience: text content of the first occurrence of an element.
pub fn tag_text<'a>(hay: &'a [u8], local: &[u8]) -> Option<&'a [u8]> {
    let (_open_start, open_end) = find_open_tag(hay, local)?;
    let close = find_close_tag(hay, open_end, local)?;
    Some(&hay[open_end..close])
}

/// Strip all tags, keeping only text content. Allocates a new buffer.
pub fn strip_tags_bytes(hay: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if hay[i] != b'<' {
            out.push(hay[i]);
            i += 1;
            continue;
        }
        // A CDATA section is text that happens to be wrapped in markup; its
        // contents belong in the output verbatim.
        if hay[i..].starts_with(b"<![CDATA[") {
            let start = i + b"<![CDATA[".len();
            match memmem::find(&hay[start..], b"]]>") {
                Some(p) => {
                    out.extend_from_slice(&hay[start..start + p]);
                    i = start + p + 3;
                }
                None => {
                    out.extend_from_slice(&hay[start..]);
                    i = hay.len();
                }
            }
            continue;
        }
        // A comment ends at `-->`, not at the first `>` inside it. Stopping at
        // the `>` would spill the rest of the comment into the text.
        if matches!(hay.get(i + 1), Some(b'!') | Some(b'?')) {
            i = skip_non_element(hay, i);
            continue;
        }
        match memchr(b'>', &hay[i..]) {
            Some(end) => i += end + 1,
            None => break, // unclosed tag → stop
        }
    }
    out
}

/// Call `f(local_name, attributes_slice)` for every start tag.
/// If `f` returns `true`, scanning stops early.
pub fn for_each_start_tag(hay: &[u8], mut f: impl FnMut(&[u8], &[u8]) -> bool) {
    let mut walk = tags(hay);
    while let Some(tag) = walk.next() {
        let Tag::Start(start) = tag else { continue };
        let Some((attrs, content)) = start.rest() else {
            return;
        };
        // This caller has just located the `>`, so tell the walk to carry on
        // from there. Without it the next `<` is looked for inside this tag's
        // own attributes — over a body of `<w mrp1="…">` elements that is most
        // of the document, scanned twice.
        walk.resume_at(content);
        if f(start.name, attrs) {
            return;
        }
    }
}

/// The `name="value"` pairs of one start tag, in order.
///
/// Quotes may be single or double. An attribute with no quoted value — a
/// boolean, or something malformed — is stepped over rather than reported: this
/// scanner is looking for values, and there is nothing to hand back.
struct Attrs<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for Attrs<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<(&'a [u8], &'a [u8])> {
        loop {
            self.rest = trim_ascii_start(self.rest);
            if self.rest.is_empty() {
                return None;
            }

            let name_len = self
                .rest
                .iter()
                .position(|&b| !is_name_char(b))
                .unwrap_or(self.rest.len());
            let (name, after_name) = self.rest.split_at(name_len);

            // The `=` may be surrounded by space, or — in this corpus — missing.
            let value_start = after_name
                .iter()
                .position(|&b| !b.is_ascii_whitespace() && b != b'=')
                .unwrap_or(after_name.len());
            let after_eq = &after_name[value_start..];

            let &quote = after_eq.first()?;
            if quote != b'"' && quote != b'\'' {
                // Nothing quoted here: skip the token and look at the next one.
                let skip = after_eq
                    .iter()
                    .position(|&b| b.is_ascii_whitespace() || b == b'>')
                    .unwrap_or(after_eq.len());
                self.rest = &after_eq[skip..];
                // A name that consumed nothing would spin here forever.
                if name.is_empty() && skip == 0 {
                    return None;
                }
                continue;
            }

            let quoted = &after_eq[1..];
            let value_len = quoted
                .iter()
                .position(|&b| b == quote)
                .unwrap_or(quoted.len());
            self.rest = quoted.get(value_len + 1..).unwrap_or(&[]);
            return Some((name, &quoted[..value_len]));
        }
    }
}

/// Extract the value of an attribute `name="value"` or `name='value'` (case-insensitive name).
pub fn attr_value<'a>(attrs: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    Attrs { rest: attrs }.find_map(|(found, value)| eq_ci(found, name).then_some(value))
}

/// Find a standalone `19xx` / `20xx` year.
///
/// The century check is what keeps sigla out of the result: `Bo 1234` is a
/// publication number, not a year.
pub fn find_year(hay: &[u8]) -> Option<[u8; 4]> {
    let mut i = 0;
    while i + 3 < hay.len() {
        let century = matches!((hay[i], hay[i + 1]), (b'1', b'9') | (b'2', b'0'));
        if century && hay[i + 2].is_ascii_digit() && hay[i + 3].is_ascii_digit() {
            let prev_ok = i == 0 || !hay[i - 1].is_ascii_digit();
            let next_ok = i + 4 >= hay.len() || !hay[i + 4].is_ascii_digit();
            if prev_ok && next_ok {
                return Some([hay[i], hay[i + 1], hay[i + 2], hay[i + 3]]);
            }
        }
        i += 1;
    }
    None
}

/// Locate `CTH` + optional whitespace + number; return the number slice.
pub fn find_cth_number(hay: &[u8]) -> Option<&[u8]> {
    let mut pos = 0;
    while pos + 3 <= hay.len() {
        let i = pos + find_ci(&hay[pos..], b"CTH")?;
        let mut j = i + 3;
        while j < hay.len() && hay[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < hay.len() && hay[j].is_ascii_digit() {
            let start = j;
            while j < hay.len() && (hay[j].is_ascii_digit() || hay[j] == b'.') {
                j += 1;
            }
            // trim trailing dots
            while j > start && hay[j - 1] == b'.' {
                j -= 1;
            }
            if j > start {
                return Some(&hay[start..j]);
            }
        }
        pos = i + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_ci_basic() {
        let h = b"xxAoHeAdERyy";
        assert_eq!(find_ci(h, b"AOHeader"), Some(2));
        assert_eq!(find_ci(h, b"nope"), None);
    }

    #[test]
    fn open_close_tag() {
        let h = b"<AO:docID id='1'>Hello</AO:docID>";
        let (o, c) = find_open_tag(h, b"docID").unwrap();
        assert_eq!(o, 0);
        assert_eq!(&h[c..c + 5], b"Hello");
        let cl = find_close_tag(h, c, b"docID").unwrap();
        assert_eq!(&h[cl..cl + 2], b"</");
        assert_eq!(tag_text(h, b"docID").unwrap(), b"Hello");
    }

    #[test]
    fn attr_and_year() {
        let a = br#"editor="FB" date="2017-03-28" src="MZ""#;
        assert_eq!(attr_value(a, b"editor"), Some(&b"FB"[..]));
        assert_eq!(attr_value(a, b"date"), Some(&b"2017-03-28"[..]));
        assert_eq!(find_year(b"x2017-03-28y"), Some(*b"2017"));
        assert_eq!(find_cth_number(b"CTH 786_XML"), Some(&b"786"[..]));
    }

    #[test]
    fn strip_tags() {
        assert_eq!(strip_tags_bytes(b"a<b>c</b>d"), b"acd");
        assert_eq!(strip_tags_bytes(b"a<b"), b"a");
        assert_eq!(strip_tags_bytes(b"plain"), b"plain");
    }

    #[test]
    fn cth_numbers() {
        assert_eq!(find_cth_number(b"CTH 786_XML"), Some(&b"786"[..]));
        assert_eq!(find_cth_number(b"cth12.1"), Some(&b"12.1"[..]));
        assert_eq!(find_cth_number(b"CTH 786."), Some(&b"786"[..]));
        assert_eq!(find_cth_number(b"CTH unknown"), None);
        assert_eq!(find_cth_number(b"CTH"), None);
    }

    #[test]
    fn start_tags_are_visited_with_attributes() {
        let mut seen: Vec<(String, String)> = Vec::new();
        for_each_start_tag(
            br#"<a x="1"/><!-- c --><b:y z='2'>text</b:y>"#,
            |local, attrs| {
                seen.push((
                    String::from_utf8_lossy(local).into_owned(),
                    String::from_utf8_lossy(attrs).into_owned(),
                ));
                false
            },
        );
        assert_eq!(
            seen,
            vec![
                ("a".to_string(), r#"x="1"/"#.to_string()),
                ("y".to_string(), "z='2'".to_string()),
            ]
        );
    }

    #[test]
    fn start_tag_scan_stops_when_asked() {
        let mut count = 0;
        for_each_start_tag(b"<a/><b/><c/>", |_, _| {
            count += 1;
            true
        });
        assert_eq!(count, 1);
    }

    /// The existing comment test used `<!-- c -->`, which has nothing inside it
    /// that looks like markup — so it passed while the scanner was stepping one
    /// byte past `<!` and finding the next `<` wherever it fell.
    #[test]
    fn a_tag_inside_a_comment_is_not_a_tag() {
        let h = br#"<!-- <docID id="ghost">nope</docID> --><docID id="real">yes</docID>"#;
        let (open, content) = find_open_tag(h, b"docID").unwrap();
        assert_eq!(&h[open..open + 14], br#"<docID id="rea"#);
        assert_eq!(tag_text(h, b"docID").unwrap(), b"yes");

        let mut seen = Vec::new();
        for_each_start_tag(h, |local, attrs| {
            seen.push(String::from_utf8_lossy(attrs).into_owned());
            assert_eq!(local, b"docID");
            false
        });
        assert_eq!(seen, vec![r#"id="real""#.to_string()]);
        let _ = content;
    }

    /// The real shape of the bug: header heuristics take the first match, so a
    /// commented-out editor would win over the live one.
    #[test]
    fn commented_out_element_does_not_outrank_the_live_one() {
        let h = br#"<AOHeader><!-- <uebern editor="OLD" date="1999-01-01"/> --><uebern editor="NEW" date="2017-03-28"/></AOHeader>"#;
        let mut editors = Vec::new();
        for_each_start_tag(h, |local, attrs| {
            if eq_ci(local, b"uebern") {
                editors.push(
                    String::from_utf8_lossy(attr_value(attrs, b"editor").unwrap()).into_owned(),
                );
            }
            false
        });
        assert_eq!(editors, vec!["NEW".to_string()]);
    }

    #[test]
    fn a_closing_tag_inside_a_comment_closes_nothing() {
        let h = br#"<note>keep<!-- </note> -->all</note>"#;
        assert_eq!(tag_text(h, b"note").unwrap(), b"keep<!-- </note> -->all");
    }

    #[test]
    fn cdata_is_text_not_markup() {
        let h = br#"<v><![CDATA[<uebern editor="ghost"/>]]></v>"#;
        let mut seen = Vec::new();
        for_each_start_tag(h, |local, _| {
            seen.push(String::from_utf8_lossy(local).into_owned());
            false
        });
        assert_eq!(seen, vec!["v".to_string()]);
        assert_eq!(
            strip_tags_bytes(h),
            br#"<uebern editor="ghost"/>"#.to_vec(),
            "CDATA contents are text and must survive stripping"
        );
    }

    #[test]
    fn declarations_and_instructions_are_stepped_over() {
        let h = br#"<?xml version="1.0"?><!DOCTYPE AO SYSTEM "ao.dtd"><AO:docID>7</AO:docID>"#;
        assert_eq!(tag_text(h, b"docID").unwrap(), b"7");

        let mut seen = Vec::new();
        for_each_start_tag(h, |local, _| {
            seen.push(String::from_utf8_lossy(local).into_owned());
            false
        });
        assert_eq!(seen, vec!["docID".to_string()]);
    }

    /// Everything after an unclosed `<!--` is comment; the scanners must stop
    /// rather than resume on the next `<` they see.
    #[test]
    fn an_unterminated_comment_swallows_the_rest() {
        let h = br#"<a/><!-- <b/>"#;
        let mut seen = Vec::new();
        for_each_start_tag(h, |local, _| {
            seen.push(String::from_utf8_lossy(local).into_owned());
            false
        });
        assert_eq!(seen, vec!["a".to_string()]);
        assert_eq!(find_open_tag(h, b"b"), None);
    }

    #[test]
    fn strip_tags_drops_a_whole_comment() {
        // The `>` in the middle used to end the "tag", spilling the rest of the
        // comment into the text.
        assert_eq!(strip_tags_bytes(b"a<!-- x > y -->b"), b"ab");
        assert_eq!(strip_tags_bytes(b"a<?pi x > y ?>b"), b"ab");
        assert_eq!(strip_tags_bytes(b"a<!-- unterminated"), b"a");
    }

    #[test]
    fn eq_ci_equal() {
        assert!(eq_ci(b".xml", b".XML"));
        assert!(eq_ci(b"AbC", b"abc"));
        assert!(!eq_ci(b"a", b"ab"));
    }

    /// One walk, one first match per name — and each name keeps its own first
    /// element rather than the earliest of any of them, which is what lets a
    /// caller prefer a spelling over a position.
    #[test]
    fn one_walk_finds_the_first_tag_of_each_name() {
        let xml = b"<root><b>second</b><a>first</a><a>again</a></root>";
        let names: [&[u8]; 3] = [b"a", b"b", b"missing"];
        let mut found = [None; 3];
        find_open_tags(xml, &names, &mut found);

        let text = |slot: Option<(usize, usize)>, name: &[u8]| {
            let (_, content) = slot?;
            let close = find_close_tag(xml, content, name)?;
            Some(String::from_utf8_lossy(&xml[content..close]).into_owned())
        };
        assert_eq!(text(found[0], b"a").as_deref(), Some("first"));
        assert_eq!(text(found[1], b"b").as_deref(), Some("second"));
        assert_eq!(found[2], None, "a name that is not there stays unanswered");
    }

    /// The same answers `find_open_tag` gives one name at a time, which is the
    /// only reason it is safe to ask for several at once.
    #[test]
    fn asking_for_several_names_agrees_with_asking_one_at_a_time() {
        let xml = b"<AOxml><AOHeader><docID>X</docID></AOHeader>                    <body><inv>early</inv><InvNr>VAT 1</InvNr></body></AOxml>";
        let names: [&[u8]; 3] = [b"InvNr", b"inv", b"docID"];
        let mut found = [None; 3];
        find_open_tags(xml, &names, &mut found);

        for (name, slot) in names.iter().zip(found) {
            assert_eq!(slot, find_open_tag(xml, name), "disagreed about {name:?}");
        }
    }

    /// Case is not part of the question, which is why one spelling is enough.
    #[test]
    fn names_are_matched_without_regard_to_case() {
        let xml = b"<root><InvNr>x</InvNr></root>";
        let names: [&[u8]; 2] = [b"invnr", b"INVNR"];
        let mut found = [None; 2];
        find_open_tags(xml, &names, &mut found);
        assert_eq!(found[0], found[1]);
        assert!(found[0].is_some());
    }
}
