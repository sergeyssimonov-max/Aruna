//! A JSON string, escaped — the crate's one answer to what that looks like.
//!
//! This crate writes JSON in three places: the catalogue, the export's
//! manifest and the page model the PDF template reads. Until 2026-10-02 the
//! first two shared an escaper and the third had its own, which wrote `\n` as
//! `\u000a`. Both forms are valid JSON; two escapers are still one change away
//! from two documents disagreeing about what a string is (finding № 10 of
//! refactor-1). The module sits beside `catalog` and `pdf` rather than inside
//! either, because neither owns the other.

/// `s` as a JSON string, quotes included, appended to `out`.
///
/// Escaped as RFC 8259 §7 requires and no further: the quote, the backslash
/// and every character below U+0020 — the three with a short form as `\n`,
/// `\r`, `\t`, the rest as `\u00XX`. Non-ASCII is written as itself. Escaping
/// it to `\u` sequences would be valid JSON and would also make a catalogue of
/// a cuneiform corpus unreadable to the person debugging it.
pub(crate) fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// [`json_str`] as a fresh `String`, for the writers that interpolate.
pub(crate) fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    json_str(s, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character RFC 8259 has an opinion about, and the ones it has
    /// none about but a careless escaper might: DEL, the two line separators
    /// JavaScript once refused, and ordinary text of this corpus.
    fn table() -> Vec<String> {
        (0u32..0x20)
            .chain([0x22, 0x5c, 0x7f, 0x2028, 0x2029])
            .filter_map(char::from_u32)
            .map(String::from)
            .chain(["Ḫattuša 𒀭 KBo 1.1".to_string(), String::new()])
            .collect()
    }

    /// A strict reader of one JSON string token (RFC 8259 §7): the whole of
    /// `token`, quotes included, or the reason it is not one.
    fn parse(token: &str) -> Result<String, String> {
        let inner = token
            .strip_prefix('"')
            .and_then(|t| t.strip_suffix('"'))
            .ok_or("not quoted")?;
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Err("an unescaped quote inside".into()),
                c if u32::from(c) < 0x20 => {
                    return Err(format!("raw U+{:04X} inside", u32::from(c)))
                }
                '\\' => match chars.next().ok_or("a lone backslash")? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if hex.len() != 4 || !hex.chars().all(|h| h.is_ascii_hexdigit()) {
                            return Err(format!("a bad \\u escape: {hex}"));
                        }
                        let unit = u32::from_str_radix(&hex, 16).map_err(|e| e.to_string())?;
                        // The escaper writes \u only below U+0020, never a
                        // surrogate half; one here is a defect, not a pair.
                        out.push(char::from_u32(unit).ok_or("a surrogate half")?);
                    }
                    other => return Err(format!("an unknown escape \\{other}")),
                },
                c => out.push(c),
            }
        }
        Ok(out)
    }

    #[test]
    fn every_character_of_the_table_is_valid_json_and_reads_back() {
        for s in table() {
            let token = json_string(&s);
            assert_eq!(parse(&token), Ok(s.clone()), "{s:?} → {token}");
        }
    }

    #[test]
    fn the_three_with_a_short_form_take_it() {
        assert_eq!(json_string("\n\r\t"), r#""\n\r\t""#);
        assert_eq!(json_string("\u{1}\u{1f}"), r#""\u0001\u001f""#);
        assert_eq!(json_string("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(json_string("\u{7f}\u{2028}𒀭"), "\"\u{7f}\u{2028}𒀭\"");
    }

    /// The reader above is the test's teeth, so it is checked to refuse.
    #[test]
    fn the_strict_reader_refuses_what_json_does_not_allow() {
        for bad in [
            "\"a\tb\"",
            "\"a\"b\"",
            "\"\\x\"",
            "\"\\u00g1\"",
            "a",
            "\"\\\"",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} was read");
        }
    }
}
