//! Is this archive entry a manuscript?
//!
//! Two answers, deliberately kept apart. [`is_manuscript_xml`] judges the name
//! and costs nothing, so [`crate::archive`] can ask it before inflating an
//! entry; [`looks_like_manuscript`] judges the bytes and is the half that
//! survives a new release of the archive. Both must say yes.

use super::{truncate_on_char_boundary, HEADER_READ_LIMIT};
use crate::xml_scan::find_ci;
use std::path::Path;

/// Whether an archive entry's *path* could be a manuscript.
///
/// The cheap half of the filter: it rejects on the name alone, before any bytes
/// are read. It cannot be the whole answer — see [`looks_like_manuscript`] —
/// because an archive can hold a file that is named like a manuscript and is
/// not one.
///
/// Every rule here is stated structurally rather than against the junk this
/// particular archive happens to contain: any `__MACOSX` segment, any segment
/// beginning with a dot. The previous version tested only the final component
/// for `._`, which worked because all 643 of this archive's stray entries were
/// AppleDouble files with that prefix — an accident of one release, not a
/// property of the format.
pub fn is_manuscript_xml(path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.last() == Some(&b'/') {
        return false; // directory entry
    }
    if bytes.len() < 4 || !bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".xml") {
        return false;
    }

    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        // `__MACOSX/` holds resource forks for the files beside it, never
        // documents; a dot-segment is a hidden file or directory.
        if segment.eq_ignore_ascii_case("__MACOSX") || segment.starts_with('.') {
            return false;
        }
    }

    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let nb = name.as_bytes();
    if nb.len() >= 8 && nb[nb.len() - 8..].eq_ignore_ascii_case(b".css.xml") {
        return false;
    }
    true
}

/// Whether an entry's *content* is a TLHdig manuscript.
///
/// The half that survives a new release of the archive. Path rules only exclude
/// the junk you already know about, and this corpus proves the gap: alongside
/// the 643 AppleDouble entries sits `CTH 813_XML_TLH/KUB 37.25.xml`, which is
/// named like every other manuscript and contains an ownCloud-encrypted blob —
/// so it parsed into a row with no editor, no year and no text, and reached both
/// the HTML and the site.
///
/// Measured over the whole corpus: 23 936 of the 23 937 real manuscripts carry
/// `AOxml`, `AOHeader` and `docID`; the one that does not is that encrypted
/// file, and none of the stray entries carries any marker at all.
///
/// If a future release moves to a shape none of these markers match, every entry
/// is rejected and the run fails with [`crate::error::ArunaError::EmptyArchive`]
/// — loudly, rather than by publishing an inventory of debris.
pub fn looks_like_manuscript(xml: &str) -> bool {
    const MARKERS: [&[u8]; 5] = [b"<AOxml", b"<AOHeader", b"<docID", b"<TEI", b"<teiHeader"];
    let window = truncate_on_char_boundary(xml, HEADER_READ_LIMIT).as_bytes();
    MARKERS.iter().any(|m| find_ci(window, m).is_some())
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::SAMPLE_FULL;
    use super::*;

    #[test]
    fn is_manuscript_xml_filters() {
        assert!(is_manuscript_xml("a/b/KBo 1.xml"));
        assert!(!is_manuscript_xml("a/b/readme.txt"));
        assert!(!is_manuscript_xml("a/b/._KBo 1.xml"));
        assert!(!is_manuscript_xml("a/b/.hidden.xml"));
    }

    /// The path rules are structural, not a list of what this one archive
    /// happens to contain. A resource fork is junk wherever it sits, and
    /// `__MACOSX/` is junk whatever its files are called — the old filter only
    /// checked the last component for `._`, and passed everything else there.
    #[test]
    fn junk_is_rejected_by_shape_rather_than_by_precedent() {
        for path in [
            "__MACOSX/CTH 1_XML/._KBo 1.xml",
            "__MACOSX/CTH 1_XML/KBo 1.xml", // named like a manuscript, still junk
            "__macosx/CTH 1_XML/KBo 1.xml", // case does not launder it
            "CTH 1_XML/.hidden/KBo 1.xml",  // a dot-directory anywhere on the path
            "CTH 1_XML/._KBo 1.xml",
            "CTH 1_XML/HPMxml.css.xml",
            "CTH 1_XML/",
            "CTH 1_XML/notes.txt",
            ".xml",
        ] {
            assert!(!is_manuscript_xml(path), "should have been rejected: {path}");
        }

        for path in [
            "TLHbasis/CTH 786_XML_HFR/KBo 17.86+.xml",
            "CTH 1_XML/İK 174-66.XML",
            "KBo 1.xml",
        ] {
            assert!(is_manuscript_xml(path), "should have been kept: {path}");
        }
    }

    /// Content is the half that survives a new release of the archive. This
    /// corpus already carries a file the path rules cannot catch: an
    /// ownCloud-encrypted blob named `KUB 37.25.xml`, which parsed into a row
    /// with no editor, no year and no text.
    #[test]
    fn a_file_named_like_a_manuscript_but_holding_something_else_is_rejected() {
        let encrypted = "HBEGIN:oc_encryption_module:OC_DEFAULT_MODULE:cipher:AES-256-CTR:\
             signed:true:useLegacyFileKey:false:encoding:binary:HEND";
        assert!(!looks_like_manuscript(encrypted));
        assert!(!looks_like_manuscript(""));
        assert!(!looks_like_manuscript("<html><body>not a manuscript</body></html>"));
        // AppleDouble entries start with a binary Mac OS X signature.
        assert!(!looks_like_manuscript("\u{0}\u{5}\u{16}\u{7}\u{0}\u{2}\u{0}\u{0}Mac OS X"));

        assert!(looks_like_manuscript(SAMPLE_FULL));
        assert!(looks_like_manuscript("<AOxml><AOHeader><docID>X</docID></AOHeader></AOxml>"));
        assert!(looks_like_manuscript("<TEI><teiHeader/></TEI>"), "TEI is accepted too");
    }

    /// The marker has to be found in the window the reader actually holds, not
    /// merely somewhere in a document that was never read that far.
    #[test]
    fn the_manuscript_marker_is_looked_for_within_the_header_window() {
        let mut late = String::from("<junk>");
        late.push_str(&"x".repeat(HEADER_READ_LIMIT));
        late.push_str("<AOHeader><docID>X</docID></AOHeader>");
        assert!(!looks_like_manuscript(&late));
    }
}
