//! The font files this program ships, and the one it puts in the package.
//!
//! **Why they travel at all.** The inventory and the PDF are built on the
//! reader's machine, not here: the application is installed on a clean Mac, it
//! downloads the corpus itself and writes both outputs there. So a font this
//! corpus needs has to arrive with the application, because nothing on that
//! machine will fetch it and this program never goes looking (4.9.6, and §3.9
//! of the specification).
//!
//! **Two different deliveries, because two different jobs.**
//!
//! The seven files below ride in the application bundle as resources and are
//! read from there, verified, when the PDF stage needs them: a PDF embeds the
//! font file, so all seven have to be present as files. macOS ships
//! `Noto Sans Cuneiform` and `STIX Two Math` itself — measured 2026-09-12,
//! byte-identical to the release files — but a PDF cannot embed a face by
//! name, so the copies come along regardless.
//!
//! `UllikummiA` is the one file the *package* carries, beside the inventory,
//! and it is compiled into this binary rather than read from the bundle. The
//! package is the console program's product as much as the window's, and it has
//! to come out the same from either; a directory that has to be found first is
//! a way for it not to. 503 160 bytes in the binary is the price of the package
//! being buildable with nothing but the executable.
//!
//! **What is never done here:** no font is installed into the reader's system,
//! none is downloaded, none is looked up by family name, and none is rewritten.
//! The terms `UllikummiA` comes under forbid modification, and
//! [`crate::export`] copies the bytes this module holds without touching them.

use crate::error::{ArunaError, Result};
use std::path::Path;

/// One file that ships, and the digest `docs/FONTS.md` records for it.
///
/// The digest is SHA-256 because that is what the table in `docs/FONTS.md`
/// carries — it is the value a reader compares against the upstream release,
/// so it is the value worth checking here. [`crate::sha256`] computes it
/// without a dependency, for the reason recorded there.
pub struct Shipped {
    /// File name inside the font directory.
    pub file: &'static str,
    /// SHA-256 as `docs/FONTS.md` records it.
    pub sha256: &'static str,
    /// Length in bytes, so a truncated file says it was truncated rather than
    /// only that the digest moved.
    pub bytes: u64,
    /// What it is here for, named in the refusal a reader sees.
    pub covers: &'static str,
}

/// The seven fonts, exactly as `docs/FONTS.md` §"What ships" lists them.
pub const FONTS: [Shipped; 7] = [
    Shipped {
        file: "UllikummiA.ttf",
        sha256: "2ca4357d66d7cde6b0785be22f4c3ed3427289fdb0330eceabe89da24c4041cf",
        bytes: 503_160,
        covers: "U+100000, which nothing on a stock machine draws",
    },
    Shipped {
        file: "NotoSansCuneiform-Regular.ttf",
        sha256: "aad6f345a2f3150aeb51706ecf1d6f62eec299ee215cb77e76f0c33e1419bba2",
        bytes: 819_980,
        covers: "the 376 cuneiform signs of the corpus",
    },
    Shipped {
        file: "STIXTwoMath-Regular.otf",
        sha256: "3a5f3f26f40d5698b3c62dd085d48d6663696a3f80825aab8b553d5097518e8c",
        bytes: 838_652,
        covers: "six editorial marks",
    },
    Shipped {
        file: "NotoSerif-Regular.ttf",
        sha256: "19e72cd8d595fae5bd74a5206f5d938512e1183d4fed7abb1ec1be1d7efa5f88",
        bytes: 712_444,
        covers: "the main face of the PDF",
    },
    Shipped {
        file: "NotoSerif-Italic.ttf",
        sha256: "749e80e313ef711f9373c6cce17c72297ef05490b3dcda7967d1d5d90bf1183f",
        bytes: 756_796,
        covers: "the main face, italic",
    },
    Shipped {
        file: "NotoSerif-Bold.ttf",
        sha256: "96656aa5cec8f1d6fd0e804c1fad397e1a1cfa082e6642124e0bda68cd8363ce",
        bytes: 747_144,
        covers: "the main face, bold",
    },
    Shipped {
        file: "NotoSerifHebrew-Regular.ttf",
        sha256: "dfd5a6aefe97a99f68fe43388342913d50bb9fbf6d3afc4d2c7725661bc4a2b1",
        bytes: 30_288,
        covers: "U+05C3",
    },
];

/// The five licence texts that travel beside the fonts.
///
/// Not digested: they are the terms, not the work, and a licence file is meant
/// to be readable rather than identical. Presence is what is checked — OFL 1.1
/// requires the text to accompany the font, and the Mainz terms are quoted with
/// the credit they ask for.
pub const LICENCES: [&str; 5] = [
    "UllikummiA-TERMS.txt",
    "OFL-NotoSansCuneiform.txt",
    "OFL-NotoSerif.txt",
    "OFL-NotoSerifHebrew.txt",
    "OFL-STIXTwo.txt",
];

/// The credit the Mainz terms require, word for word.
///
/// Quoted from `cli/resources/fonts/UllikummiA-TERMS.txt`, which quotes
/// <https://hethport.net/cuneifont/> — "The user agrees to mention the
/// following credits". It is not a paraphrase and must not become one: the
/// terms name the wording, and the outputs are where it has to appear, because
/// the reader of a package never sees this repository.
pub const CREDIT: &str =
    "Fonts created by Sylvie Vanséveren, available on the Hethitologie Portal Mainz";

/// The font the package carries, beside the inventory.
pub const PACKAGED_FONT: &str = "UllikummiA.ttf";

/// The terms that travel with it. Distribution is permitted for a scholarly,
/// non-commercial purpose and the credit has to come along; both are in here.
pub const PACKAGED_TERMS: &str = "UllikummiA-TERMS.txt";

/// The packaged font, byte for byte as the repository holds it.
pub const PACKAGED_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/UllikummiA.ttf");

/// The terms file, byte for byte.
pub const PACKAGED_TERMS_BYTES: &[u8] = include_bytes!("../resources/fonts/UllikummiA-TERMS.txt");

/// Check every shipped file in `dir` against the table above.
///
/// Called with the directory the files actually arrived in — the application's
/// resource directory in an installed bundle — and with nothing else. There is
/// no search: a caller that does not know where its resources are cannot be
/// helped by this module guessing, and guessing is how a system font gets used
/// in place of the one that was checked.
///
/// The three ways it can fail are three different sentences, because they call
/// for three different actions: a file that is not there means an incomplete
/// install, a file of the wrong length means a truncated one, and a file that
/// is the right length and the wrong digest means a substituted one.
pub fn verify_dir(dir: &Path) -> Result<()> {
    for font in &FONTS {
        verify_one(dir, font)?;
    }
    for licence in LICENCES {
        let path = dir.join(licence);
        if !path.is_file() {
            return Err(ArunaError::FontMissing {
                path,
                covers: "the terms the font beside it is distributed under",
            });
        }
    }
    Ok(())
}

/// Check one file: present, the recorded length, the recorded digest.
fn verify_one(dir: &Path, font: &Shipped) -> Result<()> {
    let path = dir.join(font.file);
    let Ok(meta) = std::fs::metadata(&path) else {
        return Err(ArunaError::FontMissing {
            path,
            covers: font.covers,
        });
    };
    if meta.len() != font.bytes {
        return Err(ArunaError::FontAltered {
            path,
            expected: font.sha256,
            found: format!("{} bytes, not {}", meta.len(), font.bytes),
        });
    }
    let digest = crate::sha256::sha256_file(&path).map_err(ArunaError::io(&path))?;
    if digest != font.sha256 {
        return Err(ArunaError::FontAltered {
            path,
            expected: font.sha256,
            found: digest,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The directory of the repository, which is what the bundle is built from.
    fn tree() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/fonts")
    }

    /// The table and the tree agree — which is the same question the bundle
    /// will be asked at run time, asked here at build time.
    #[test]
    fn every_shipped_file_matches_the_table() {
        verify_dir(&tree()).expect("the repository's own font directory verifies");
    }

    /// The bytes compiled in are the bytes on disk, so the package cannot ship
    /// a font that differs from the one the terms describe.
    #[test]
    fn the_packaged_font_is_the_file_on_disk() {
        let on_disk = std::fs::read(tree().join(PACKAGED_FONT)).expect("read the font");
        assert_eq!(PACKAGED_FONT_BYTES, on_disk.as_slice());
        assert_eq!(
            crate::sha256::sha256_hex(PACKAGED_FONT_BYTES),
            FONTS[0].sha256
        );
        let terms = std::fs::read(tree().join(PACKAGED_TERMS)).expect("read the terms");
        assert_eq!(PACKAGED_TERMS_BYTES, terms.as_slice());
    }

    /// The credit is quoted, not written: it has to be in the terms file
    /// verbatim, or it is a paraphrase of a licence condition.
    #[test]
    fn the_credit_is_quoted_from_the_terms() {
        let terms = String::from_utf8(PACKAGED_TERMS_BYTES.to_vec()).expect("terms are UTF-8");
        // The terms file wraps at 76 columns, so the sentence is compared with
        // its runs of whitespace flattened: the words and their order are what
        // is quoted, and a line break is not a word.
        let flat = terms.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            flat.contains(CREDIT),
            "the credit is not in the terms file word for word"
        );
    }

    /// Missing and altered are different refusals with different words, because
    /// they call for different repairs.
    #[test]
    fn missing_and_altered_are_told_apart() {
        let dir = tempfile::tempdir().expect("temp");
        let err = verify_dir(dir.path()).expect_err("an empty directory cannot verify");
        let text = format!("{err}");
        assert!(text.contains("UllikummiA.ttf"), "{text}");
        assert!(text.contains("is not there"), "{text}");

        for font in &FONTS {
            std::fs::copy(tree().join(font.file), dir.path().join(font.file)).expect("copy");
        }
        for licence in LICENCES {
            std::fs::copy(tree().join(licence), dir.path().join(licence)).expect("copy");
        }
        verify_dir(dir.path()).expect("a full copy verifies");

        // Same length, one byte different: the length check cannot see it and
        // the digest must.
        let path = dir.path().join("NotoSerif-Regular.ttf");
        let mut bytes = std::fs::read(&path).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).expect("write");
        let text = format!(
            "{}",
            verify_dir(dir.path()).expect_err("a substituted file")
        );
        assert!(text.contains("NotoSerif-Regular.ttf"), "{text}");
        assert!(text.contains("has been replaced"), "{text}");

        // Truncated: the same file, short. The message says how short.
        std::fs::write(&path, &bytes[..bytes.len() - 100]).expect("truncate");
        let text = format!("{}", verify_dir(dir.path()).expect_err("a truncated file"));
        assert!(text.contains("bytes, not 712444"), "{text}");
    }
}
