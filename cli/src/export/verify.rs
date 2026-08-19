//! Proof that normalising a document changed nothing it was not allowed to.
//!
//! The permit list is the whole of what [`super::normalize`] may do:
//!
//! ```text
//! DROP_BOM        a leading U+FEFF        it would sit in front of the declaration
//! DROP_PI   xml            the declaration, replaced by a canonical one
//! DROP_PI   xml-stylesheet HPMxml.css is not shipped in the package
//! ADD       declaration    <?xml version="1.0" encoding="UTF-8"?>
//! REFLOW    prologue space whitespace between prologue instructions becomes one newline
//! ```
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

/// The processing instructions the normaliser is permitted to drop.
pub const DROPPED: [&str; 2] = ["xml", "xml-stylesheet"];

/// The declaration it is permitted to add.
pub const DECLARATION: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

/// Which permitted changes one document actually underwent.
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
    let added_declaration = !source_pis.iter().any(|pi| target_of(pi) == "xml");

    // Every instruction the source carried is either kept verbatim or is one
    // the permit list names.
    let mut dropped = Vec::new();
    for pi in &source_pis {
        let target = target_of(pi);
        if normal_pis.iter().any(|kept| kept == pi) {
            continue;
        }
        if DROPPED.contains(&target.as_str()) {
            dropped.push(target);
            continue;
        }
        return Err(format!(
            "instruction <?{target}…?> was dropped and is not on the permit list"
        ));
    }

    // …and nothing was invented: every kept instruction was in the source.
    for pi in &normal_pis {
        if pi.starts_with(b"<?xml ") {
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
fn split_prologue(bytes: &[u8]) -> (&[u8], Vec<Vec<u8>>, bool) {
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
        let Some(end) = find(rest, b"?>").map(|at| at + 2) else {
            break;
        };
        pis.push(rest[..end].to_vec());
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

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The name after `<?`.
fn target_of(pi: &[u8]) -> String {
    let rest = pi.get(2..).unwrap_or_default();
    let end = rest
        .iter()
        .position(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'?'))
        .unwrap_or(rest.len());
    String::from_utf8_lossy(&rest[..end]).into_owned()
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
