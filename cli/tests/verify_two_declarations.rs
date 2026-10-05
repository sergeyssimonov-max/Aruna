//! What `verify::compare` answers for a source with two `<?xml?>` declarations.
//!
//! No document of the corpus carries two, and no fixture did: the two lookups in
//! `compare`, for the encoding and for the version, each take the first
//! declaration *that names the attribute*, not the attribute of the first
//! declaration, and with two declarations those differ. This test holds the
//! answers the code gives today (2026-10-05), so that a restructuring of
//! `compare` cannot change them unnoticed. It records behaviour, it does not
//! endorse it: the second case lets a declared `version="1.1"` through.

use aruna::export::{normalize_into, verify};

/// The canonical declaration and the body, which is what the normaliser makes
/// of all three sources below.
const NORMALISED: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<AOxml><a/></AOxml>";

fn compare(source: &[u8]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    normalize_into(source, &mut out);
    assert_eq!(String::from_utf8_lossy(&out), NORMALISED);
    verify::compare(source, &out).map(|report| report.applied())
}

#[test]
fn two_xml_declarations_are_read_the_way_compare_reads_them_today() {
    // The first declaration names no encoding, the second names Latin-1: the
    // encoding is taken from the second, and the document is refused.
    assert_eq!(
        compare(
            b"<?xml version=\"1.0\"?>\n<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<AOxml><a/></AOxml>"
        ),
        Err("the source declares encoding=\"ISO-8859-1\" and the canonical declaration says UTF-8; \
             the bytes would be kept and their meaning changed"
            .to_string())
    );

    // The first declaration says 1.0, the second 1.1: the version is taken from
    // the first, the second is dropped as an instruction, and it goes through.
    assert_eq!(
        compare(
            b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<?xml version=\"1.1\"?>\n<AOxml><a/></AOxml>"
        ),
        Ok(vec!["DROP_PI xml".to_string()])
    );

    // Two identical declarations, neither the canonical one: both dropped.
    assert_eq!(
        compare(b"<?xml version=\"1.0\"?>\n<?xml version=\"1.0\"?>\n<AOxml><a/></AOxml>"),
        Ok(vec!["DROP_PI xml".to_string(), "DROP_PI xml".to_string()])
    );
}
