//! Proof that normalising a document changed nothing it was not allowed to.
//!
//! The permit list is the whole of what [`super::normalize`] may do, and it is
//! written once — in [`DROPPED`] and the constants beside it — rather than in
//! prose here, in the normaliser, and again in the manifest. It used to be all
//! three, and they had already drifted: the normaliser dropped an instruction
//! whose target matched without regard to case, and this module recognised only
//! the exact lower-case spelling, so a document writing `<?XML-STYLESHEET?>`
//! would have had a permitted removal reported as distortion and stopped the
//! build.
//!
//! What is *not* shared is the mechanism. This module walks a prologue with its
//! own code rather than calling the normaliser's, because a checker that reuses
//! the thing it checks agrees with its bugs.
//!
//! Everything from the first byte that is not part of the prologue onwards must
//! be identical, byte for byte. Not "equivalent", not "the same once both sides
//! are normalised" — identical. Comparing normalised forms would hide exactly
//! the corruption this exists to catch, and 9 % of this corpus is not in NFC,
//! so it would hide a great deal of it.
//!
//! Every document is checked before it is written, not a sample of them. A
//! document whose non-distortion is not proven is not published, and the build
//! stops rather than publishing the rest.

use crate::xml_scan::find_exact;

/// The processing instructions the normaliser is permitted to drop, and why.
///
/// The reason travels with the target because the manifest publishes both, and
/// a rule whose explanation lives somewhere else is a rule that gets dropped
/// from the explanation.
pub const DROPPED: [(&str, &str); 2] = [
    ("xml", "the declaration, replaced by a canonical one"),
    ("xml-stylesheet", "HPMxml.css is not part of the package"),
];

/// The declaration it is permitted to add.
pub const DECLARATION: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

/// The names each permitted change is counted and advertised under.
///
/// The manifest's `applied` map is keyed by these and its `permitted` list is
/// rendered from them, so a change cannot be counted under a name the manifest
/// never offers — which is what two independent lists joined by a shared prefix
/// invited.
pub const DROP_BOM: &str = "DROP_BOM";
/// See [`DROP_BOM`].
pub const ADD_DECLARATION: &str = "ADD declaration";
/// See [`DROP_BOM`].
pub const REFLOW_PROLOGUE: &str = "REFLOW prologue whitespace";

/// The name a dropped instruction is counted under.
pub fn drop_pi(target: &str) -> String {
    format!("DROP_PI {target}")
}

/// Whether `target` names an instruction the normaliser may drop.
///
/// Without regard to case, because that is how the normaliser decides.
pub fn is_dropped(target: &[u8]) -> bool {
    DROPPED
        .iter()
        .any(|(name, _)| target.eq_ignore_ascii_case(name.as_bytes()))
}

/// Which permitted changes one document actually underwent.
#[derive(Debug)]
pub struct Report {
    pub dropped: Vec<String>,
    pub added_declaration: bool,
    pub reflowed: bool,
}

/// Compare a document with its normalised form, allowing only the permit list.
pub fn compare(source: &[u8], normalised: &[u8]) -> Result<Report, String> {
    let (source_body, source_pis, source_space) = split_prologue(strip_bom(source));
    let (normal_body, normal_pis, _) = split_prologue(normalised);

    // The one that matters: everything the prologue does not cover is the
    // document, and it must survive untouched.
    if source_body != normal_body {
        return Err(distortion(source_body, normal_body));
    }

    // The declaration must be the canonical one, written once.
    if !normalised.starts_with(DECLARATION) {
        return Err("the output does not begin with the canonical declaration".into());
    }

    // Replacing the declaration is on the permit list; replacing what it says
    // about the encoding is not. The body is copied byte for byte, so swapping
    // `encoding="ISO-8859-1"` for `encoding="UTF-8"` leaves every byte where it
    // was and changes what all of them mean — and the byte comparison above
    // would call that no distortion, because by that measure it is none.
    //
    // No declaration at all is safe: XML already reads an undeclared document
    // as UTF-8, so writing that down states what was true rather than
    // something new.
    if let Some(encoding) = source_pis.iter().find_map(|pi| {
        (target_of(pi).eq_ignore_ascii_case(b"xml")).then(|| declared_encoding(pi))?
    }) {
        if !is_utf8_name(encoding) {
            return Err(format!(
                "the source declares encoding=\"{}\" and the canonical declaration says UTF-8; \
                 the bytes would be kept and their meaning changed",
                String::from_utf8_lossy(encoding)
            ));
        }
    }
    let added_declaration = !source_pis
        .iter()
        .any(|pi| target_of(pi).eq_ignore_ascii_case(b"xml"));

    // Every instruction the source carried is either kept verbatim or is one
    // the permit list names.
    let mut dropped = Vec::new();
    for pi in &source_pis {
        if normal_pis.iter().any(|kept| kept == pi) {
            continue;
        }
        let target = target_of(pi);
        if is_dropped(target) {
            dropped.push(String::from_utf8_lossy(target).into_owned());
            continue;
        }
        return Err(format!(
            "instruction <?{}…?> was dropped and is not on the permit list",
            String::from_utf8_lossy(target)
        ));
    }

    // …and nothing was invented: every kept instruction was in the source.
    for pi in &normal_pis {
        if DECLARATION.starts_with(pi) {
            continue; // the declaration written above
        }
        if !source_pis.iter().any(|had| had == pi) {
            return Err("an instruction appears in the output that was not in the source".into());
        }
    }

    Ok(Report {
        dropped,
        added_declaration,
        reflowed: source_space,
    })
}

/// Where the prologue ends: the instructions before it, and whether the
/// whitespace between them was anything but a single newline.
/// Borrowed from the input rather than copied out of it: both documents outlive
/// the comparison, and copying every instruction of every document was some two
/// hundred thousand small allocations per package for data that is read once.
fn split_prologue(bytes: &[u8]) -> (&[u8], Vec<&[u8]>, bool) {
    let mut i = 0usize;
    let mut pis = Vec::new();
    let mut reflowed = false;

    loop {
        let space = leading_space(&bytes[i..]);
        if space > 0 && bytes[i..i + space] != *b"\n" {
            reflowed = true;
        }
        i += space;
        let rest = &bytes[i..];
        if !rest.starts_with(b"<?") {
            break;
        }
        let Some(end) = find_exact(rest, b"?>").map(|at| at + 2) else {
            break;
        };
        pis.push(&rest[..end]);
        i += end;
    }
    (&bytes[i..], pis, reflowed)
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)
}

fn leading_space(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        .unwrap_or(bytes.len())
}

/// The `encoding` pseudo-attribute of an XML declaration, if it has one.
///
/// Read here rather than borrowed from a shared helper for the same reason the
/// prologue walk is: this module checks the normaliser, and a checker that
/// reads a value with the normaliser's own code cannot disagree with it.
fn declared_encoding(pi: &[u8]) -> Option<&[u8]> {
    let at = pi
        .windows(b"encoding".len())
        .position(|w| w.eq_ignore_ascii_case(b"encoding"))?;
    let rest = &pi[at + b"encoding".len()..];
    let eq = rest.iter().position(|b| *b == b'=')?;
    let after = &rest[eq + 1..];
    let open = after.iter().position(|b| *b == b'"' || *b == b'\'')?;
    let quote = after[open];
    let close = after[open + 1..].iter().position(|b| *b == quote)?;
    Some(&after[open + 1..open + 1 + close])
}

/// Whether an encoding name means UTF-8.
///
/// The two spellings the standard registers for it, and nothing else: a name
/// this does not recognise is treated as "not UTF-8", which errs towards
/// refusing to touch a document rather than towards changing it.
fn is_utf8_name(name: &[u8]) -> bool {
    name.eq_ignore_ascii_case(b"utf-8") || name.eq_ignore_ascii_case(b"utf8")
}

/// The name after `<?`.
///
/// Borrowed from the instruction rather than copied out of it: this is asked of
/// every instruction of every document, and the answer is almost always thrown
/// away after one comparison.
fn target_of(pi: &[u8]) -> &[u8] {
    let rest = pi.get(2..).unwrap_or_default();
    let end = rest
        .iter()
        .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'?'))
        .unwrap_or(rest.len());
    &rest[..end]
}

/// Where two bodies first differ, and what is on either side of it.
///
/// A byte offset alone is not enough to act on; a distortion in a corpus of
/// cuneiform is usually invisible in a diff of rendered text.
fn distortion(source: &[u8], normalised: &[u8]) -> String {
    let at = source
        .iter()
        .zip(normalised)
        .position(|(a, b)| a != b)
        .unwrap_or(source.len().min(normalised.len()));
    let from = at.saturating_sub(30);
    let window =
        |bytes: &[u8]| String::from_utf8_lossy(&bytes[from..bytes.len().min(at + 30)]).into_owned();
    format!(
        "content differs at byte {at} (source {} bytes, output {} bytes)\n      source: …{}…\n      output: …{}…",
        source.len(),
        normalised.len(),
        window(source),
        window(normalised)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical output for a document that carried no prologue.
    fn declared(body: &[u8]) -> Vec<u8> {
        let mut out = DECLARATION.to_vec();
        out.extend_from_slice(body);
        out
    }

    /// The refusals below are the whole reason this module exists, and until
    /// now not one of them had been seen to fire. Every test in the suite fed
    /// [`compare`] a document and the normaliser's own output for it, so the
    /// checker was only ever observed agreeing. A checker that has never said
    /// no is not known to be able to: the branches are reached by handing it
    /// output a *broken* normaliser would have produced, which is the only
    /// state in which they are supposed to be reached at all.
    ///
    /// What is asserted is the refusal, not its wording — except where the
    /// message is the deliverable, which is [`distortion`]: a byte offset in a
    /// corpus of cuneiform is not something anyone can act on alone.
    #[test]
    fn a_body_that_changed_is_refused_and_the_message_says_where() {
        let source = b"<AOxml><w>ta-ba-ar-na</w></AOxml>";
        let broken = declared(b"<AOxml><w>ta-ba-ar-NA</w></AOxml>");

        let why = compare(source, &broken).expect_err("a changed body must not pass");

        assert!(
            why.contains("content differs at byte 19"),
            "the offset of the first differing byte is missing: {why}"
        );
        assert!(
            why.contains("source: ") && why.contains("output: "),
            "both sides of the difference must be shown: {why}"
        );
    }

    /// The lengths are printed because a truncation differs from a substitution
    /// in no other visible way when the tail is what went missing.
    #[test]
    fn a_body_that_was_cut_short_is_refused_and_the_message_says_both_lengths() {
        let source = b"<AOxml><w>ta-ba-ar-na</w></AOxml>";
        let broken = declared(b"<AOxml><w>ta-ba-ar-na</w>");

        let why = compare(source, &broken).expect_err("a truncated body must not pass");

        assert!(
            why.contains("source 33 bytes, output 25 bytes"),
            "the two lengths are what name a truncation: {why}"
        );
    }

    /// The declaration is written by the normaliser on every document, so its
    /// absence means the output did not come from the normaliser this checks.
    #[test]
    fn output_that_does_not_open_with_the_canonical_declaration_is_refused() {
        let source = b"<AOxml/>";

        let why = compare(source, source).expect_err("an undeclared output must not pass");

        assert!(
            why.contains("canonical declaration"),
            "the refusal must name what is missing: {why}"
        );
    }

    /// The permit list names two instructions. An instruction that vanished and
    /// is neither of them was deleted by something that had no permission to,
    /// and the document is short a piece of itself that nobody looked at.
    #[test]
    fn an_instruction_dropped_from_outside_the_permit_list_is_refused() {
        let source = b"<?some-tool note=\"keep me\"?><AOxml/>";
        let broken = declared(b"<AOxml/>");

        let why = compare(source, &broken).expect_err("an unpermitted removal must not pass");

        assert!(
            why.contains("some-tool") && why.contains("permit list"),
            "the refusal must name the instruction it lost: {why}"
        );
    }

    /// The other direction, and the one a byte comparison of bodies cannot see:
    /// the prologue is the one region where the output is allowed to differ, so
    /// something invented there travels with the document unchallenged.
    #[test]
    fn an_instruction_invented_in_the_output_is_refused() {
        let source = b"<AOxml/>";
        let mut broken = DECLARATION.to_vec();
        broken.extend_from_slice(b"<?invented?>\n<AOxml/>");

        let why = compare(source, &broken).expect_err("an invented instruction must not pass");

        assert!(
            why.contains("was not in the source"),
            "the refusal must say the output gained something: {why}"
        );
    }

    /// An unterminated `<?` is not an instruction, and the permit list has
    /// nothing to say about it: the prologue ends where it begins, and the
    /// bytes are body from there on. Both walks agree on that, so the document
    /// carries its broken opening into the package untouched — which is the
    /// right answer for a corpus in which 210 documents are not well-formed.
    #[test]
    fn an_unterminated_instruction_is_body_rather_than_prologue() {
        let source = b"<?truncated<AOxml/>";
        let out = declared(source);

        let report = compare(source, &out).expect("a malformed prologue is not a distortion");

        assert!(report.dropped.is_empty(), "nothing was dropped");
        assert!(report.added_declaration, "the source declared nothing");
    }
}
