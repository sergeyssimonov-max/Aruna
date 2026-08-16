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

    /// Ids are integers even though JSON calls them numbers.
    #[test]
    fn only_whole_non_negative_numbers_are_ids() {
        assert_eq!(parse("20328284").and_then(|v| v.as_u64()), Some(20328284));
        assert_eq!(parse("1.5").and_then(|v| v.as_u64()), None);
        assert_eq!(parse("-3").and_then(|v| v.as_u64()), None);
        assert_eq!(parse("1e3").and_then(|v| v.as_u64()), Some(1000));
    }
}
