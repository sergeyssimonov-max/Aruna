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
    is_ascii_alpha(b)
        || b.is_ascii_digit()
        || matches!(b, b':' | b'-' | b'_' | b'.')
}

/// Local part of a possibly prefixed XML Name (`AO:docID` → `docID`).
fn local_part(name: &[u8]) -> &[u8] {
    match memchr(b':', name) {
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

/// Find the opening tag `<…local…>` (with optional namespace prefix).
/// Returns `(start_of_tag, end_of_opening_tag)` so the caller can slice attributes or text.
pub fn find_open_tag(hay: &[u8], local: &[u8]) -> Option<(usize, usize)> {
    let mut pos = 0;
    while let Some(rel) = memchr(b'<', &hay[pos..]) {
        let i = pos + rel;
        if i + 1 >= hay.len() {
            break;
        }
        if hay[i + 1] == b'!' || hay[i + 1] == b'?' {
            pos = skip_non_element(hay, i);
            continue;
        }
        if hay[i + 1] == b'/' {
            pos = i + 1;
            continue;
        }

        // Read the name, then compare only its local part.
        let (name, j) = element_name(hay, i + 1);
        if eq_ci(name, local) {
            // Find the end of the opening tag.
            if let Some(end) = memchr(b'>', &hay[j..]) {
                return Some((i, j + end + 1));
            }
            return None;
        }
        pos = i + 1;
    }
    None
}

/// Find the matching closing tag `</…local…>` starting search from `from`.
pub fn find_close_tag(hay: &[u8], from: usize, local: &[u8]) -> Option<usize> {
    let mut pos = from;
    while let Some(rel) = memchr(b'<', &hay[pos..]) {
        let i = pos + rel;
        if i + 1 >= hay.len() {
            break;
        }
        // A comment inside the element may hold a `</…>` that closes nothing.
        if hay[i + 1] == b'!' || hay[i + 1] == b'?' {
            pos = skip_non_element(hay, i);
            continue;
        }
        if hay[i + 1] == b'/' {
            let (name, _) = element_name(hay, i + 2);
            if eq_ci(name, local) {
                return Some(i);
            }
        }
        pos = i + 1;
    }
    None
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
    let mut pos = 0;
    while let Some(rel) = memchr(b'<', &hay[pos..]) {
        let i = pos + rel;
        if i + 1 >= hay.len() {
            break;
        }
        let next = hay[i + 1];
        if next == b'!' || next == b'?' {
            pos = skip_non_element(hay, i);
            continue;
        }
        if next == b'/' {
            pos = i + 1;
            continue;
        }

        let (local, j) = element_name(hay, i + 1);

        // attributes run until '>' or '/>'
        let end = match memchr(b'>', &hay[j..]) {
            Some(e) => j + e,
            None => break,
        };
        let mut attr_start = j;
        while attr_start < end && hay[attr_start].is_ascii_whitespace() {
            attr_start += 1;
        }
        let attrs = &hay[attr_start..end];

        if f(local, attrs) {
            return;
        }
        pos = end + 1;
    }
}

/// Extract the value of an attribute `name="value"` or `name='value'` (case-insensitive name).
pub fn attr_value<'a>(attrs: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut pos = 0;
    while pos < attrs.len() {
        // skip whitespace
        while pos < attrs.len() && attrs[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= attrs.len() {
            break;
        }

        let name_start = pos;
        while pos < attrs.len() && is_name_char(attrs[pos]) {
            pos += 1;
        }
        let found_name = &attrs[name_start..pos];

        // skip whitespace and '='
        while pos < attrs.len() && (attrs[pos].is_ascii_whitespace() || attrs[pos] == b'=') {
            pos += 1;
        }
        if pos >= attrs.len() {
            break;
        }

        let quote = attrs[pos];
        if quote != b'"' && quote != b'\'' {
            // malformed or boolean attribute
            while pos < attrs.len() && !attrs[pos].is_ascii_whitespace() && attrs[pos] != b'>' {
                pos += 1;
            }
            continue;
        }
        pos += 1; // skip opening quote
        let value_start = pos;
        while pos < attrs.len() && attrs[pos] != quote {
            pos += 1;
        }
        let value = &attrs[value_start..pos];
        if pos < attrs.len() {
            pos += 1; // skip closing quote
        }

        if eq_ci(found_name, name) {
            return Some(value);
        }
    }
    None
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
}
