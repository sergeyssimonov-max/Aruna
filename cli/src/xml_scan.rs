//! Byte scanners for TLHdig / AOxml header heuristics.
//!
//! The parser never builds a DOM. It only needs a handful of tags and
//! attributes from the first few KiB of each document. Candidate positions
//! are located with `memchr`; everything after that is a plain scalar loop.
//!
//! Design priorities (in order):
//! 1. Correctness and total behaviour (malformed input yields None / empty, never panics)
//! 2. Readability
//! 3. Acceptable performance on short fragments
//!
//! Micro-optimisations without measurements are intentionally avoided.

use memchr::{memchr, memchr2, memmem};

/// ASCII case-fold for A–Z only.
pub const fn ascii_lower(b: u8) -> u8 {
    b | 0x20
}

/// True when `b` is ASCII alphabetic (A–Z / a–z).
fn is_ascii_alpha(b: u8) -> bool {
    ascii_lower(b).wrapping_sub(b'a') < 26
}

/// True when `b` may appear inside an XML Name (local or prefix).
fn is_name_char(b: u8) -> bool {
    is_ascii_alpha(b)
        || b.is_ascii_digit()
        || b == b':'
        || b == b'-'
        || b == b'_'
        || b == b'.'
}

/// Case-insensitive ASCII search for `needle` in `hay`.
///
/// First-byte candidates are located with `memchr2` (upper+lower).
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
    // For non-alpha first bytes, upper==lower after fold — use single memchr.
    let use_pair = first_lo != first_up && is_ascii_alpha(first);

    let mut pos = 0usize;
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

/// Case-insensitive equality of two equal-length slices (ASCII).
pub fn eq_ci(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(&x, &y)| ascii_lower(x) == ascii_lower(y))
}

/// Find `needle` with substring search (case-sensitive).
pub fn find_exact(hay: &[u8], needle: &[u8]) -> Option<usize> {
    memmem::find(hay, needle)
}

/// Find first `needle` among several exact needles; returns (offset, which).
pub fn find_first_of(hay: &[u8], needles: &[&[u8]]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (idx, n) in needles.iter().enumerate() {
        if let Some(pos) = memmem::find(hay, n) {
            best = match best {
                None => Some((pos, idx)),
                Some((bp, _)) if pos < bp => Some((pos, idx)),
                other => other,
            };
        }
    }
    best
}

/// Locate an opening tag whose local name equals `local`
/// (optional `prefix:` is accepted).
///
/// Returns `(tag_open_at, content_start)` where `content_start` is the index
/// just after the closing `>` of the start-tag.
pub fn find_open_tag(hay: &[u8], local: &[u8]) -> Option<(usize, usize)> {
    let mut pos = 0usize;
    while pos < hay.len() {
        let rel = memchr(b'<', &hay[pos..])?;
        let i = pos + rel;
        // Skip closing tags, comments, PI, declarations.
        let next = *hay.get(i + 1)?;
        if next == b'/' || next == b'!' || next == b'?' {
            pos = i + 2;
            continue;
        }
        // Parse name
        let name_start = i + 1;
        let mut name_end = name_start;
        while name_end < hay.len() && is_name_char(hay[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            pos = i + 1;
            continue;
        }
        let name = &hay[name_start..name_end];
        let local_part = match memchr(b':', name) {
            Some(c) => &name[c + 1..],
            None => name,
        };
        if eq_ci(local_part, local) {
            // Find end of start-tag `>`, aware of quotes.
            let gt = find_tag_close(hay, name_end)?;
            return Some((i, gt + 1));
        }
        pos = name_end;
    }
    None
}

/// Find `>` that closes a start/end tag, skipping quoted attribute values.
pub fn find_tag_close(hay: &[u8], mut i: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    while i < hay.len() {
        let b = hay[i];
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
        } else {
            match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => return Some(i),
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Find a matching end-tag `</local>` or `</prefix:local>` (case-insensitive local).
/// Search starts at `from`. Returns byte index of `'<'` of the end-tag.
pub fn find_close_tag(hay: &[u8], from: usize, local: &[u8]) -> Option<usize> {
    let slice = hay.get(from..)?;
    let mut pos = 0usize;
    while pos < slice.len() {
        let rel = memchr(b'<', &slice[pos..])?;
        let i = pos + rel;
        if slice.get(i + 1) != Some(&b'/') {
            pos = i + 1;
            continue;
        }
        let name_start = i + 2;
        let mut name_end = name_start;
        while name_end < slice.len() && is_name_char(slice[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            pos = i + 1;
            continue;
        }
        let name = &slice[name_start..name_end];
        let local_part = match memchr(b':', name) {
            Some(c) => &name[c + 1..],
            None => name,
        };
        if eq_ci(local_part, local) {
            return Some(from + i);
        }
        pos = name_end;
    }
    None
}

/// Extract text content of the first element with the given local name.
pub fn tag_text<'a>(hay: &'a [u8], local: &[u8]) -> Option<&'a [u8]> {
    let (_, content_start) = find_open_tag(hay, local)?;
    let close = find_close_tag(hay, content_start, local)?;
    Some(&hay[content_start..close])
}

/// Strip tags from a small fragment, keeping only text content.
pub fn strip_tags_bytes(hay: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(hay.len());
    let mut i = 0usize;
    while i < hay.len() {
        if let Some(rel) = memchr(b'<', &hay[i..]) {
            let lt = i + rel;
            out.extend_from_slice(&hay[i..lt]);
            match memchr(b'>', &hay[lt..]) {
                Some(r) => i = lt + r + 1,
                None => {
                    // unclosed tag: drop the rest
                    return out;
                }
            }
        } else {
            out.extend_from_slice(&hay[i..]);
            break;
        }
    }
    out
}

/// For each start-tag in `hay`, call `f(local_name, attrs)`.
/// Stops early if `f` returns `true`.
pub fn for_each_start_tag(hay: &[u8], mut f: impl FnMut(&[u8], &[u8]) -> bool) {
    let mut pos = 0usize;
    while pos < hay.len() {
        let Some(rel) = memchr(b'<', &hay[pos..]) else {
            break;
        };
        let i = pos + rel;
        let next = match hay.get(i + 1) {
            Some(b) => *b,
            None => break,
        };
        if next == b'/' || next == b'!' || next == b'?' {
            pos = i + 2;
            continue;
        }
        let name_start = i + 1;
        let mut name_end = name_start;
        while name_end < hay.len() && is_name_char(hay[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            pos = i + 1;
            continue;
        }
        let Some(gt) = find_tag_close(hay, name_end) else {
            break;
        };
        // attrs between name_end and gt (trim leading whitespace)
        let mut attr_start = name_end;
        while attr_start < gt && hay[attr_start].is_ascii_whitespace() {
            attr_start += 1;
        }
        let name = &hay[name_start..name_end];
        let local = match memchr(b':', name) {
            Some(c) => &name[c + 1..],
            None => name,
        };
        let attrs = &hay[attr_start..gt];
        if f(local, attrs) {
            return;
        }
        pos = gt + 1;
    }
}

/// Extract `attr="value"` or `attr='value'` (case-insensitive attribute name).
pub fn attr_value<'a>(attrs: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut pos = 0usize;
    while pos < attrs.len() {
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
        if pos == name_start {
            pos += 1;
            continue;
        }
        let aname = &attrs[name_start..pos];
        while pos < attrs.len() && attrs[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= attrs.len() || attrs[pos] != b'=' {
            continue;
        }
        pos += 1;
        while pos < attrs.len() && attrs[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= attrs.len() {
            break;
        }
        let q = attrs[pos];
        if q != b'"' && q != b'\'' {
            // unquoted value
            let vstart = pos;
            while pos < attrs.len() && !attrs[pos].is_ascii_whitespace() && attrs[pos] != b'/' {
                pos += 1;
            }
            if eq_ci(aname, name) {
                return Some(&attrs[vstart..pos]);
            }
            continue;
        }
        pos += 1;
        let vstart = pos;
        while pos < attrs.len() && attrs[pos] != q {
            pos += 1;
        }
        let vend = pos;
        if pos < attrs.len() {
            pos += 1;
        }
        if eq_ci(aname, name) {
            return Some(&attrs[vstart..vend]);
        }
    }
    None
}

/// Find first year 19xx / 20xx.
pub fn find_year(hay: &[u8]) -> Option<[u8; 4]> {
    let mut i = 0usize;
    while i + 4 <= hay.len() {
        let Some(rel) = memchr2(b'1', b'2', &hay[i..]) else {
            return None;
        };
        i += rel;
        if i + 4 > hay.len() {
            return None;
        }
        let b0 = hay[i];
        let b1 = hay[i + 1];
        let b2 = hay[i + 2];
        let b3 = hay[i + 3];
        if b0 == b'1' && b1 == b'9' && b2.is_ascii_digit() && b3.is_ascii_digit() {
            let prev_ok = i == 0 || !hay[i - 1].is_ascii_digit();
            let next_ok = i + 4 >= hay.len() || !hay[i + 4].is_ascii_digit();
            if prev_ok && next_ok {
                return Some([b0, b1, b2, b3]);
            }
        }
        if b0 == b'2' && b1 == b'0' && b2.is_ascii_digit() && b3.is_ascii_digit() {
            let prev_ok = i == 0 || !hay[i - 1].is_ascii_digit();
            let next_ok = i + 4 >= hay.len() || !hay[i + 4].is_ascii_digit();
            if prev_ok && next_ok {
                return Some([b0, b1, b2, b3]);
            }
        }
        i += 1;
    }
    None
}

/// Locate `CTH` + optional whitespace + number; returns the number slice.
pub fn find_cth_number(hay: &[u8]) -> Option<&[u8]> {
    let mut pos = 0usize;
    while pos + 3 <= hay.len() {
        let Some(rel) = find_ci(&hay[pos..], b"CTH") else {
            return None;
        };
        let i = pos + rel;
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
        let out = strip_tags_bytes(b"a<b>c</b>d");
        assert_eq!(out, b"acd");
    }

    #[test]
    fn eq_ci_equal() {
        assert!(eq_ci(b".xml", b".XML"));
        assert!(eq_ci(b"AbC", b"abc"));
        assert!(!eq_ci(b"a", b"ab"));
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
}
