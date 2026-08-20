//! Where a document goes, and what the inventory calls it.
//!
//! Pure: strings in, strings out, no filesystem and no archive. This is the one
//! description of an output path — the writer, the inventory's links and the
//! validator all come here, because three descriptions of one path is how a
//! package ends up with an inventory pointing at files that are not where it
//! says they are.

use std::path::{Path, PathBuf};

/// Where one fragment is written, relative to the package root.
///
/// `CTH 786/KBo 17.86+.xml`. Both components go through the rules below, so a
/// siglum that cannot be a filename as written is still placed deterministically
/// rather than refused or silently mangled.
pub fn output_path(group: &str, file_stem: &str) -> PathBuf {
    PathBuf::from(dir_component(group)).join(format!("{}.xml", path_component(file_stem)))
}

/// The directory name for a CTH group.
pub fn dir_component(group: &str) -> String {
    path_component(group)
}

/// A single path component that is safe on a filesystem and still readable.
///
/// The corpus makes this necessary rather than theoretical: 29 sigla are
/// excavation numbers written with a slash — `Bo 2023/23`, `544/f`, `93/w+` —
/// and a slash cannot be in a filename anywhere. It becomes `%2F`, which is
/// what a URL would call it anyway, so the name still reads as the siglum does.
///
/// Everything a filesystem or a shell could misread goes the same way, and a
/// name that would otherwise be `.` or `..`, or start with a dot, is prefixed:
/// a hidden directory is not what a group of manuscripts should be, and `..` is
/// how a path escapes the folder it belongs in.
pub fn path_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 8);
    for ch in raw.chars() {
        match ch {
            '/' | '\\' | ':' => push_percent(&mut out, ch as u8),
            c if (c as u32) < 0x20 || c == '\u{7f}' => push_percent(&mut out, c as u8),
            c => out.push(c),
        }
    }
    let trimmed = out.trim_end_matches([' ', '.']);
    let out = if trimmed.is_empty() { &out } else { trimmed }.to_string();
    if out.is_empty() || out.starts_with('.') {
        format!("_{out}")
    } else {
        out
    }
}

/// Where this document's PDF will go, from the path its XML took.
///
/// The same name with a different extension, derived here rather than by the
/// converter: two naming rules for one document is how a package and the thing
/// built from it stop agreeing. It is also what lets a collision between two
/// PDFs be found while the package is built — `Bo 2023%2F23.xml` and
/// `Bo 2023%2F23.pdf` escape the slash identically, so a pair that collides in
/// one collides in the other.
pub fn pdf_path(xml: &Path) -> PathBuf {
    xml.with_extension("pdf")
}

/// The relative URL for a path inside the package.
///
/// Percent-encoded per component, and only the characters that need it: the
/// name stays legible in a browser's address bar, which matters for a package
/// people open by double-clicking. `%2F` produced by [`path_component`] survives
/// as `%252F` — encoding it once more is correct, because by then the percent is
/// a literal character in a filename rather than an escape.
pub fn href(path: &Path) -> String {
    let mut url = String::from(".");
    for component in path.components() {
        url.push('/');
        url.push_str(&encode_uri_component(
            &component.as_os_str().to_string_lossy(),
        ));
    }
    url
}

/// Percent-encode one path segment for a URL.
fn encode_uri_component(segment: &str) -> String {
    // Escaping only grows a segment, and the corpus's spaces and brackets mean
    // most of them do grow.
    let mut out = String::with_capacity(segment.len() + 8);
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => push_percent(&mut out, *other),
        }
    }
    out
}

/// One percent-escape, written straight into the buffer.
///
/// `format!("%{byte:02X}")` allocates a `String` and goes through `core::fmt`
/// for three characters. That is charged per escaped byte, and the package
/// builds some seventy thousand of these URLs — one per document in the
/// inventory, again in each group page, and again in the manifest.
fn push_percent(out: &mut String, byte: u8) {
    const HEX: [u8; 16] = *b"0123456789ABCDEF";
    out.push('%');
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0xF) as usize] as char);
}

/// Turn one URL segment back into the name it stands for.
///
/// The inverse of [`encode_uri_component`], and the validator's half of the
/// round trip: a link is only checked against the file it names if the name can
/// be recovered from it.
pub fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Read the two hex digits as bytes. Slicing the `&str` here would panic
        // the moment a `%` is followed by a multi-byte character — `%aé` cuts
        // through the middle of the `é` — and this function exists to survive
        // input nobody vouched for.
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi * 16 + lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One hex digit, or `None` if it is not one.
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The path a relative href points at, or `None` if it points outside.
///
/// Used by the validator rather than trusted from it: a link that is absolute,
/// or that walks upwards, is refused here instead of being resolved and then
/// found to be outside the package.
pub fn resolve(href: &str) -> Option<PathBuf> {
    let rest = href.strip_prefix("./")?;
    if rest.is_empty() {
        return None;
    }
    let mut path = PathBuf::new();
    for segment in rest.split('/') {
        let decoded = percent_decode(segment);
        if decoded.is_empty() || decoded == "." || decoded == ".." || decoded.contains('/') {
            return None;
        }
        path.push(decoded);
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slash_in_a_siglum_becomes_a_name_a_filesystem_accepts() {
        // 29 of this corpus's sigla are excavation numbers written like this.
        assert_eq!(path_component("Bo 2023/23"), "Bo 2023%2F23");
        assert_eq!(path_component("544/f"), "544%2Ff");
        assert_eq!(
            output_path("CTH 96", "544/f"),
            PathBuf::from("CTH 96/544%2Ff.xml")
        );
    }

    #[test]
    fn a_component_can_never_escape_the_package() {
        for hostile in ["..", ".", "../../etc/passwd", "/etc/passwd", ".hidden", ""] {
            let component = path_component(hostile);
            assert!(!component.contains('/'), "{hostile:?} kept a separator");
            assert!(!component.starts_with('.'), "{hostile:?} stayed hidden");
            assert_ne!(component, "..");
            assert!(!component.is_empty());
        }
    }

    #[test]
    fn hrefs_are_relative_and_encoded_once_per_component() {
        assert_eq!(
            href(&output_path("CTH 786", "KBo 17.86+")),
            "./CTH%20786/KBo%2017.86%2B.xml"
        );
        // The escape a slash became is a literal percent in the file name, so
        // the URL escapes the percent in turn.
        assert_eq!(
            href(&output_path("CTH 96", "544/f")),
            "./CTH%2096/544%252Ff.xml"
        );
        assert!(href(&PathBuf::from("CTH 1")).starts_with("./"));
    }

    /// The round trip is what lets the validator check a link against a file:
    /// whatever the name was, it has to come back out of the URL unchanged.
    #[test]
    fn every_name_survives_the_trip_through_a_url() {
        for name in [
            "KBo 17.86+",
            "Bo 2023/23",
            "İK 174-66",
            "KUB 19.25(+)",
            "𒀀 cuneiform",
            "KBo 26.25 (sumerisch-akkadisch; mit Unterstrich)",
        ] {
            let path = output_path("CTH 1", name);
            let resolved = resolve(&href(&path)).expect("a link this package wrote");
            assert_eq!(resolved, path, "{name:?} did not survive the round trip");
        }
    }

    #[test]
    fn a_link_that_leaves_the_package_does_not_resolve() {
        for hostile in [
            "../secret",
            "./../secret",
            "/etc/passwd",
            "https://example.com/x",
            "./CTH%201/../../etc",
            "./",
            "CTH 1/x.xml",
        ] {
            assert!(resolve(hostile).is_none(), "{hostile:?} resolved");
        }
    }
}
