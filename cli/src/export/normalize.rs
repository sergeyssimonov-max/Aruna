//! Turning one archive document into the one the package ships.
//!
//! Pure: bytes in, bytes out. No filesystem, no archive, no record — which is
//! what lets the transform be checked against a handful of literals instead of
//! against a 71 MiB corpus.

/// Normalise one document for the package.
///
/// Two changes, and deliberately no others. The corpus is scholarly text with
/// `xml:space="preserve"` on its root, so reindenting or collapsing whitespace
/// would be editing the data; nothing here touches the markup between the root
/// tags.
///
/// * The stylesheet instruction is dropped. Documents carry
///   `<?xml-stylesheet href="HPMxml.css" type="text/css"?>`, and `HPMxml.css`
///   lives beside them in the archive but is not part of this package — 35 % of
///   a sample carried the reference, so a third of the package would have
///   pointed at a file that is not there.
/// * A declaration is added. Almost no document has one (1 of 60 sampled), and
///   a file that is going to be opened on its own should say what it is and how
///   it is encoded. An existing declaration is replaced rather than doubled.
///
/// The result is a document that differs from the archive's copy, which is the
/// point: the package holds normalised documents, not the originals.
pub fn normalize_document(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 40);
    normalize_into(bytes, &mut out);
    out
}

/// As [`normalize_document`], into a buffer the caller keeps.
///
/// The export writes 24 000 documents in a row; growing a fresh `Vec` for each
/// is 24 000 allocations for a buffer that is the same size every time. `out`
/// is appended to, so the caller clears it rather than this deciding for them.
pub fn normalize_into(bytes: &[u8], out: &mut Vec<u8>) {
    const DECLARATION: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

    let mut body = bytes;
    // A leading BOM would sit in front of the declaration and make it invalid.
    if let Some(rest) = body.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        body = rest;
    }

    out.reserve(bytes.len() + DECLARATION.len());
    out.extend_from_slice(DECLARATION);

    // Leading processing instructions and whitespace are what this walks; the
    // first thing that is neither ends the prologue and is copied untouched.
    let mut i = leading_whitespace(body);
    loop {
        let rest = &body[i..];
        let Some(end) = processing_instruction_end(rest) else {
            break;
        };
        let pi = &rest[..end];
        if !is_dropped_instruction(pi) {
            out.extend_from_slice(pi);
            out.push(b'\n');
        }
        i += end;
        i += leading_whitespace(&body[i..]);
    }

    out.extend_from_slice(&body[i..]);
}

/// Whether a document carries the stylesheet instruction the package drops.
///
/// Counted by the export so the number it reports is what was actually removed
/// rather than an estimate.
pub fn carries_stylesheet(bytes: &[u8]) -> bool {
    find(bytes, b"<?xml-stylesheet").is_some()
}

/// How many leading bytes are XML whitespace.
fn leading_whitespace(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        .unwrap_or(bytes.len())
}

/// The length of a processing instruction at the start of `bytes`, if there is one.
fn processing_instruction_end(bytes: &[u8]) -> Option<usize> {
    if !bytes.starts_with(b"<?") {
        return None;
    }
    let close = find(bytes, b"?>")?;
    Some(close + 2)
}

/// Whether a processing instruction is one the package does without.
///
/// The declaration, because one is written afresh, and the stylesheet, because
/// the stylesheet is not here. Anything else a document carries is its own and
/// is kept: dropping instructions by shape rather than by name is how a
/// normaliser starts deleting things nobody looked at.
fn is_dropped_instruction(pi: &[u8]) -> bool {
    let target = pi
        .get(2..)
        .map(|rest| {
            let end = rest
                .iter()
                .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'?'))
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .unwrap_or(b"");
    target.eq_ignore_ascii_case(b"xml") || target.eq_ignore_ascii_case(b"xml-stylesheet")
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stylesheet_instruction_goes_and_a_declaration_arrives() {
        let source = br#"<?xml-stylesheet href="HPMxml.css" type="text/css"?><AOxml><a/></AOxml>"#;
        let out = normalize_document(source);
        let text = String::from_utf8(out).expect("utf-8");

        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(
            !text.contains("xml-stylesheet"),
            "the dangling reference survived"
        );
        assert!(
            text.contains("<AOxml><a/></AOxml>"),
            "the document itself was altered"
        );
    }

    #[test]
    fn a_document_that_already_declares_itself_is_not_declared_twice() {
        let source = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<AOxml/>";
        let text = String::from_utf8(normalize_document(source)).expect("utf-8");
        assert_eq!(text.matches("<?xml version").count(), 1);
        assert!(text.ends_with("<AOxml/>"));
    }

    /// Instructions that are neither of the two named ones belong to the
    /// document and stay in it.
    #[test]
    fn an_unfamiliar_instruction_is_left_alone() {
        let source = b"<?some-tool note=\"keep me\"?><AOxml/>";
        let text = String::from_utf8(normalize_document(source)).expect("utf-8");
        assert!(text.contains("<?some-tool note=\"keep me\"?>"));
    }

    /// The body is scholarly text under `xml:space="preserve"`; the normaliser
    /// must not touch a byte of it.
    #[test]
    fn the_markup_after_the_prologue_is_untouched() {
        let body = "<AOxml xml:space=\"preserve\"><w>  a  </w>\n<w>𒀀</w></AOxml>";
        let source = format!("<?xml-stylesheet href=\"x.css\"?>{body}");
        let text = String::from_utf8(normalize_document(source.as_bytes())).expect("utf-8");
        assert!(text.ends_with(body), "the body changed: {text:?}");
    }

    #[test]
    fn a_byte_order_mark_does_not_end_up_inside_the_declaration() {
        let mut source = vec![0xEF, 0xBB, 0xBF];
        source.extend_from_slice(b"<AOxml/>");
        let out = normalize_document(&source);
        assert!(out.starts_with(b"<?xml version"));
        assert!(!out.windows(3).any(|w| w == [0xEF, 0xBB, 0xBF]));
    }
}
