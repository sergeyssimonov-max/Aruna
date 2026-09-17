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

/// Nothing in this program reaches into the machine's own font directories.
///
/// The whole point of carrying the files is that the machine the inventory and
/// the PDF are built on is not this one: it is a clean Mac that has never had
/// `UllikummiA` and never will. A path into `/System/Library/Fonts` or
/// `~/Library/Fonts` in the program would quietly undo that — it would work
/// here, on the desk where every font is installed, and fail or, worse, find
/// something else on the reader's.
///
/// `examples/font_coverage` is exempt and is the reason the exemption is named
/// rather than assumed: measuring what a system draws is exactly what it is
/// for, and it is an example, not the program.
#[test]
fn no_source_file_reaches_for_a_system_font_directory() {
    let roots = [
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../src-tauri/src"),
    ];
    let forbidden = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "~/Library/Fonts",
        "/usr/share/fonts",
    ];

    let mut stack: Vec<PathBuf> = roots.to_vec();
    let mut read = 0usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .expect("read a source directory")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read a source file");
            read += 1;
            for needle in forbidden {
                assert!(
                    !text.contains(needle),
                    "{} names {needle}: the fonts ship with the application and are read from it",
                    path.display()
                );
            }
        }
    }
    assert!(read > 20, "only {read} source files were scanned");
}

/// Every shipped file is the file `docs/FONTS.md` records, and the five licence
/// texts are beside them.
///
/// The table lives in `aruna::fonts`; this asks it about the tree the bundle is
/// built from, which is the build-time half of the pair. The run-time half is
/// in the application's `setup`, against the directory inside the installed
/// bundle — a file replaced after the build is invisible here and visible
/// there, which is why there are two.
#[test]
fn the_seven_fonts_and_five_licences_are_the_recorded_files() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts");
    aruna::fonts::verify_dir(&dir).expect("the repository's font directory verifies");

    assert_eq!(aruna::fonts::FONTS.len(), 7);
    assert_eq!(aruna::fonts::LICENCES.len(), 5);
    for font in &aruna::fonts::FONTS {
        let path = dir.join(font.file);
        assert_eq!(
            aruna::sha256::sha256_file(&path).expect("digest"),
            font.sha256,
            "{} is not the file docs/FONTS.md records",
            font.file
        );
    }

    // The digests are in the document too, so a reader comparing against
    // upstream and a machine checking at run time read the same numbers.
    let doc =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/FONTS.md"))
            .expect("read docs/FONTS.md");
    for font in &aruna::fonts::FONTS {
        assert!(
            doc.contains(font.sha256),
            "docs/FONTS.md does not carry the digest of {}",
            font.file
        );
    }
}
