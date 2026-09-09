//! The one font that ships in this repository, held to the bytes it arrived as.
//!
//! `UllikummiA.ttf` draws `U+100000`, a cuneiform sign with no Unicode code
//! point, which occurs 927 times in the corpus and which nothing on a stock
//! macOS machine draws. It is the only file here that is not ours: it is
//! copyright Sylvie Vanséveren, and the Hethitologie-Portal Mainz terms that
//! come with it forbid modification and distribution in modified form.
//! `cli/resources/fonts/UllikummiA-TERMS.txt` carries those terms verbatim.
//!
//! So this is not a drift check, it is a licence check. A font file edited in
//! place — subsetted to save space, "fixed", re-saved by a tool that opened it
//! — would put this repository outside the terms it distributes the file under,
//! and would do it silently: no test reads a font, nothing renders a PDF yet,
//! and the sign it draws is one nobody here can spot by eye.
//!
//! The digest is MD5 because MD5 is the digest this crate has (`aruna::md5`,
//! written for Zenodo's checksums). It answers "are these the bytes we
//! recorded", which is the whole question. The upstream pin is the SHA-256 in
//! `docs/FONTS.md` and in the terms file, and it is what a fresh download is
//! compared against; this test compares the tree against itself.

use aruna::md5::md5_file;
use std::path::{Path, PathBuf};

/// The font as it came from the portal, checked 2026-09-09 against both the
/// copy installed in `~/Library/Fonts` and the recorded upstream SHA-256
/// `2ca4357d66d7cde6b0785be22f4c3ed3427289fdb0330eceabe89da24c4041cf`.
const ULLIKUMMI_A_MD5: &str = "d0baacffa8037099eda4926979d92704";

/// Version 1.003, 503 160 bytes. Length is checked as well as the digest so a
/// truncated checkout — an interrupted clone, a filter that mangled a binary —
/// says what happened rather than only that the digest moved.
const ULLIKUMMI_A_BYTES: u64 = 503_160;

fn font(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/fonts")
        .join(name)
}

#[test]
fn the_bundled_font_is_the_file_the_terms_describe() {
    let path = font("UllikummiA.ttf");
    assert!(
        path.is_file(),
        "{} is missing: the repository distributes this font, and docs/FONTS.md \
         tells a reader it is here",
        path.display()
    );

    let size = std::fs::metadata(&path).expect("font metadata").len();
    assert_eq!(
        size, ULLIKUMMI_A_BYTES,
        "UllikummiA.ttf is {size} bytes, not {ULLIKUMMI_A_BYTES}"
    );

    let digest = md5_file(&path).expect("digest the font");
    assert_eq!(
        digest, ULLIKUMMI_A_MD5,
        "UllikummiA.ttf has been modified. The terms it is distributed under \
         forbid that, and forbid distributing it in modified form: restore the \
         file from the upstream package named in resources/fonts/UllikummiA-TERMS.txt"
    );
}

#[test]
fn the_terms_travel_with_the_font() {
    let terms = font("UllikummiA-TERMS.txt");
    assert!(
        terms.is_file(),
        "{} is missing: the font may not be distributed without its terms and \
         the credit they require",
        terms.display()
    );

    let text = std::fs::read_to_string(&terms).expect("read the terms");
    for required in [
        "Fonts created by Sylvie Vanséveren",
        "may not\n    be modified",
        "2ca4357d66d7cde6b0785be22f4c3ed3427289fdb0330eceabe89da24c4041cf",
    ] {
        assert!(
            text.contains(required),
            "the terms file no longer carries {required:?}"
        );
    }
}
