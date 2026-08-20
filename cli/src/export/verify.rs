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
