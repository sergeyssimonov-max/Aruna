//! Just enough JSON to read a Zenodo record.
//!
//! Hand-written for the same reason `catalog.rs` writes JSON by hand: this
//! program carries no serialisation dependency, and the document being read is
//! a handful of fields from a known service. What it is *not* is a shortcut —
//! a regex over the response would misread the first nested `"key"` it met, so
//! this parses properly and refuses anything malformed.
//!
//! Total by construction: every function returns `Option`, nothing panics, and
//! depth is capped so a hostile document cannot exhaust the stack.

use std::collections::BTreeMap;

/// A parsed JSON value. Numbers are `f64` as the format defines them; `as_u64`
/// is where the one integer this module needs is recovered.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    /// Member of an object, or `None` for anything else.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(map) => map.get(key),
            _ => None,
        }
    }

    /// Element of an array, or `None` for anything else.
    pub fn at(&self, index: usize) -> Option<&Json> {
        match self {
            Json::Array(items) => items.get(index),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    /// The value as a whole, non-negative integer.
    ///
    /// Record ids arrive as JSON numbers, which are floating point by
    /// definition; anything fractional or negative is not an id.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(n) if n.is_finite() && *n >= 0.0 && n.fract() == 0.0 => Some(*n as u64),
            _ => None,
        }
    }
}

/// How deep a document may nest.
///
/// Zenodo's records reach four or five levels; a thousand is far beyond
/// anything real and far below what would overflow the stack.
const MAX_DEPTH: usize = 1000;

/// Parse a whole document, or `None` if it is not valid JSON.
///
/// Trailing content is rejected: half a document that happens to start well is
/// not a document.
pub fn parse(text: &str) -> Option<Json> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = p.value(0)?;
    p.skip_whitespace();
    p.at.eq(&p.bytes.len()).then_some(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Consume `lit` if it is next.
    fn literal(&mut self, lit: &[u8]) -> bool {
        if self.bytes[self.at..].starts_with(lit) {
            self.at += lit.len();
            return true;
        }
        false
    }

    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_whitespace();
        match self.peek()? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(Json::String),
            b't' => self.literal(b"true").then_some(Json::Bool(true)),
            b'f' => self.literal(b"false").then_some(Json::Bool(false)),
            b'n' => self.literal(b"null").then_some(Json::Null),
            _ => self.number(),
        }
    }

    fn object(&mut self, depth: usize) -> Option<Json> {
        self.at += 1; // '{'
        let mut map = BTreeMap::new();
        self.skip_whitespace();
        if self.peek()? == b'}' {
            self.at += 1;
            return Some(Json::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = self.string()?;
            self.skip_whitespace();
            if self.peek()? != b':' {
                return None;
            }
            self.at += 1;
            let value = self.value(depth + 1)?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek()? {
                b',' => self.at += 1,
                b'}' => {
                    self.at += 1;
                    return Some(Json::Object(map));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<Json> {
        self.at += 1; // '['
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek()? == b']' {
            self.at += 1;
            return Some(Json::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.skip_whitespace();
            match self.peek()? {
                b',' => self.at += 1,
                b']' => {
                    self.at += 1;
                    return Some(Json::Array(items));
                }
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        if self.peek()? != b'"' {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            match self.peek()? {
                b'"' => {
                    self.at += 1;
                    return Some(out);
                }
                b'\\' => {
                    self.at += 1;
                    let escape = self.peek()?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return None,
                    }
                }
                // A raw control character is not allowed in a JSON string.
                c if c < 0x20 => return None,
                _ => {
                    // Multi-byte UTF-8 passes through whole: the input is a
                    // `&str`, so its boundaries are already valid.
                    let start = self.at;
                    self.at += 1;
                    while self.peek().is_some_and(|b| b & 0xC0 == 0x80) {
                        self.at += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).ok()?);
                }
            }
        }
    }

    /// `\uXXXX`, including the surrogate pair that spells a character above the
    /// basic plane — which is how a JSON document carries cuneiform.
    fn unicode_escape(&mut self) -> Option<char> {
        let first = self.hex4()?;
        if !(0xD800..0xDC00).contains(&first) {
            return char::from_u32(first);
        }
        // A high surrogate means the character continues in a second escape.
        if !self.literal(b"\\u") {
            return None;
        }
        let second = self.hex4()?;
        if !(0xDC00..0xE000).contains(&second) {
            return None;
        }
        char::from_u32(0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00))
    }

    fn hex4(&mut self) -> Option<u32> {
        let digits = self.bytes.get(self.at..self.at + 4)?;
        let text = std::str::from_utf8(digits).ok()?;
        let value = u32::from_str_radix(text, 16).ok()?;
        self.at += 4;
        Some(value)
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
        text.parse().ok().map(Json::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shapes_a_record_is_made_of() {
        let v = parse(r#"{"a": 1, "b": "x", "c": [1, 2], "d": {"e": true}, "f": null}"#).unwrap();
        assert_eq!(v.get("a").and_then(Json::as_u64), Some(1));
        assert_eq!(v.get("b").and_then(Json::as_str), Some("x"));
        assert_eq!(v.get("c").and_then(|c| c.at(1)).and_then(Json::as_u64), Some(2));
        assert_eq!(v.get("d").and_then(|d| d.get("e")), Some(&Json::Bool(true)));
        assert_eq!(v.get("f"), Some(&Json::Null));
        assert_eq!(v.get("missing"), None);
    }

    /// A nested `"key"` must not be mistaken for a top-level one — which is
    /// exactly what a regex over the response would do.
    #[test]
    fn nesting_is_respected() {
        let v = parse(r#"{"files": [{"key": "inner"}], "key": "outer"}"#).unwrap();
        assert_eq!(v.get("key").and_then(Json::as_str), Some("outer"));
        assert_eq!(
            v.get("files").and_then(|f| f.at(0)).and_then(|f| f.get("key")).and_then(Json::as_str),
            Some("inner")
        );
    }

    #[test]
    fn escapes_and_unicode_survive() {
        let v = parse(r#"{"s": "a\"b\\c\ndéሀ0"}"#);
        assert_eq!(v.unwrap().get("s").and_then(Json::as_str), Some("a\"b\\c\ndéሀ0"));

        // A surrogate pair spells cuneiform.
        let v = parse(r#"{"s": "𒀀"}"#).unwrap();
        assert_eq!(v.get("s").and_then(Json::as_str), Some("𒀀"));
    }

    #[test]
    fn malformed_documents_are_refused() {
        for text in [
            "",
            "{",
            "}",
            "{\"a\"}",
            "{\"a\": }",
            "{\"a\": 1,}",
            "[1, 2",
            "\"unterminated",
            "{\"a\": 1} trailing",
            r#"{"a": "\q"}"#,
            r#"{"a": "\ud808"}"#, // high surrogate with nothing after it
        ] {
            assert!(parse(text).is_none(), "accepted {text:?}");
        }
    }

    /// Depth is capped, so a document built to exhaust the stack does not.
    #[test]
    fn runaway_nesting_is_refused_rather_than_fatal() {
        let deep = "[".repeat(MAX_DEPTH + 10) + &"]".repeat(MAX_DEPTH + 10);
        assert!(parse(&deep).is_none());

        let fine = "[".repeat(20) + &"]".repeat(20);
        assert!(parse(&fine).is_some());
    }

    /// A real record, near enough: every shape the parser has to walk.
    const REAL: &str = r#"{"created":"2026-05-21T10:00:00+00:00","id":20328284,
      "conceptrecid":"15459133","doi":"10.5281/zenodo.20328284","revision":4,
      "files":[{"id":"744d460e","key":"TLHbasisONLINE25_1_ZENODO_Beta_03.zip",
        "size":74449198,"checksum":"md5:f9acbc8db3111cc7dd88d82f7819a912",
        "links":{"self":"https://zenodo.org/api/records/20328284/files/x/content"}}],
      "metadata":{"title":"Thesaurus Linguarum Hethaeorum digitalis","publication_date":"2026-05-21",
        "license":{"id":"cc-by-4.0"},"creators":[{"name":"Rieken, E."},{"name":"Schwemer, D."}],
        "keywords":["Hittite","cuneiform"],"notes":null,"open":true,"version":1.0},
      "stats":{"downloads":1234,"views":5678.0},"swh":{}}"#;

    /// Every prefix of a real response must be refused, never fatal.
    ///
    /// This is the shape a truncated transfer actually takes, and the parser
    /// reads bytes from the network by hand — the one place in this program
    /// where an index off the end would be a crash rather than an error.
    #[test]
    fn every_truncation_of_a_real_record_is_survivable() {
        for cut in 0..=REAL.len() {
            let Some(prefix) = REAL.get(..cut) else {
                continue; // not a character boundary
            };
            let parsed = parse(prefix);
            if cut == REAL.len() {
                assert!(parsed.is_some(), "the whole document must parse");
            } else {
                assert!(parsed.is_none(), "a prefix is not a document: {cut}");
            }
        }
    }

    /// Corruption anywhere in the response must be refused or read, never fatal.
    ///
    /// Deterministic rather than random: every byte position, replaced with the
    /// characters that actually break parsers — quotes, braces, backslashes,
    /// control bytes.
    #[test]
    fn corruption_at_any_position_is_survivable() {
        let bytes = REAL.as_bytes();
        for at in 0..bytes.len() {
            for replacement in [b'"', b'{', b'}', b'[', b']', b'\\', b':', b',', b'0', 0x01, 0x7f]
            {
                let mut broken = bytes.to_vec();
                broken[at] = replacement;
                // Only valid UTF-8 reaches the parser: the caller holds a String.
                if let Ok(text) = std::str::from_utf8(&broken) {
                    let _ = parse(text); // must not panic; either answer is fine
                }
            }
        }
    }

    /// Insertions and deletions, which truncation and single-byte edits miss.
    #[test]
    fn edits_anywhere_are_survivable() {
        let bytes = REAL.as_bytes();
        let mut seed = 0x2026_0816_u64;
        let mut next = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize
        };
        for _ in 0..2000 {
            let mut broken = bytes.to_vec();
            let at = next() % broken.len();
            match next() % 3 {
                0 => {
                    broken.remove(at);
                }
                1 => broken.insert(at, b"\"{}[]:,\\ "[next() % 9]),
                _ => broken.truncate(at),
            }
            if let Ok(text) = std::str::from_utf8(&broken) {
                let _ = parse(text);
            }
        }
    }

    /// Documents built to break a parser rather than to be read.
    #[test]
    fn hostile_documents_are_refused_rather_than_fatal() {
        let cases = vec![
            "\u{feff}{}".to_string(),                 // byte-order mark
            "{\"a\":".to_string() + &"[".repeat(500) + "1" + &"]".repeat(500) + "}",
            format!("{{\"a\":\"{}\"}}", "x".repeat(100_000)), // a very long string
            format!("{{\"a\":{}}}", "9".repeat(400)),        // a number past f64
            "{\"a\":1e999}".to_string(),                    // infinity
            "{\"a\":-1e999}".to_string(),
            "{\"\":\"empty key\"}".to_string(),
            "{\"a\":\"\\ud83d\\ude00\"}".to_string(),        // a valid surrogate pair
            "{\"a\":\"\\udc00\\ud800\"}".to_string(),        // surrogates the wrong way round
            "{\"a\":\"\\u0000\"}".to_string(),                // escaped NUL
            "[".repeat(100_000),                              // deeper than the cap
            "{\"a\":{\"a\":{\"a\":1}}}".to_string(),
        ];
        for text in cases {
            let _ = parse(&text); // no panic, no stack overflow
        }
    }

    /// A response of the wrong shape entirely: HTML, an error document, a bare
    /// value. Each must parse or refuse without pretending to be a record.
    #[test]
    fn responses_that_are_not_records_do_not_pretend_to_be() {
        assert!(parse("<html><body>502 Bad Gateway</body></html>").is_none());
        let error = parse(r#"{"status":404,"message":"PID does not exist."}"#).unwrap();
        assert_eq!(error.get("id"), None, "an error document has no record id");
        assert_eq!(parse("[]").unwrap().get("id"), None);
        assert_eq!(parse("42").unwrap().get("id"), None);
    }

    /// Ids are integers even though JSON calls them numbers.
    #[test]
    fn only_whole_non_negative_numbers_are_ids() {
        assert_eq!(parse("20328284").and_then(|v| v.as_u64()), Some(20328284));
        assert_eq!(parse("1.5").and_then(|v| v.as_u64()), None);
        assert_eq!(parse("-3").and_then(|v| v.as_u64()), None);
        assert_eq!(parse("1e3").and_then(|v| v.as_u64()), Some(1000));
    }
}
