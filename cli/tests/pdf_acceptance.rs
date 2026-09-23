//! Preparations for accepting a PDF, before there is a PDF to accept.
//!
//! `docs/PDF-ACCEPTANCE.md` names the instruments a converter will be held to,
//! and every one of them ships with macOS: `xmllint --c14n` for the structure
//! of the XML side and `xsltproc` for an independent extraction — both already
//! driven by `xml_contract.rs` and `document_model.rs` — and `sips` for turning
//! a page into pixels, which a visual check needs and which nothing here drove
//! until now. A criterion whose instrument has never been run is a sentence,
//! not a check; this makes it a check of the instrument, so the day a PDF
//! exists the only new question is the PDF.
//!
//! No converter, no PDF crate and no snapshot crate: those are chosen with the
//! first PDF (specification 3.8). The document here is written by hand, a few
//! hundred bytes whose cross-reference offsets are computed rather than typed.
//!
//! A missing `sips` skips and says so; `ARUNA_REQUIRE_FIXTURE=1` turns that
//! into a failure, as it does for the other instruments.

use std::path::Path;
use std::process::{Command, Stdio};

/// A one-page PDF, `side` points square, painted black.
///
/// The smallest document a rasteriser has to get right: a catalog, a page
/// tree, one page with a content stream, and a cross-reference table whose
/// offsets are the bytes actually written.
fn one_black_page(side: u32) -> Vec<u8> {
    let content = format!("0 0 0 rg 0 0 {side} {side} re f\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {side} {side}] /Contents 4 0 R /Resources << >> >>"
        ),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (n, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", n + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// Whether `sips` is here, failing instead of skipping when the run requires
/// its instruments.
fn sips_present() -> bool {
    let here = Command::new("sips")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !here {
        assert!(
            std::env::var_os("ARUNA_REQUIRE_FIXTURE").is_none(),
            "ARUNA_REQUIRE_FIXTURE is set but sips is not installed"
        );
        eprintln!(
            "sips is not installed: the rasterisation check did not run, and nothing is claimed"
        );
    }
    here
}

/// `pdf` rasterised to PNG at `png`, and whether `sips` said it succeeded.
fn rasterise(pdf: &Path, png: &Path) -> bool {
    Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(pdf)
        .arg("--out")
        .arg(png)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && png.is_file()
}

/// The width and height of an image, as `sips` reads them back.
fn pixel_size(image: &Path) -> Option<(u32, u32)> {
    let out = Command::new("sips")
        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
        .arg(image)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key)?.trim().parse::<u32>().ok())
    };
    Some((value("pixelWidth:")?, value("pixelHeight:")?))
}

/// **A page becomes pixels of the size it declares, and a broken file does
/// not.**
///
/// The visual criterion of `PDF-ACCEPTANCE.md` §3 starts from a raster of each
/// page. What is held here is the instrument, not a converter: `sips` turns a
/// page of known size into an image of that size, and refuses a file whose
/// structure is cut off. The negative control is the second half — an
/// instrument that produced an image from anything would pass the first half
/// and prove nothing.
#[test]
fn sips_rasterises_a_page_to_its_size_and_refuses_a_broken_file() {
    if !sips_present() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");

    // Two sizes, so the raster is shown to follow the page and not to be a
    // fixed default: at the 72 dpi `sips` renders at, a point is a pixel.
    for side in [144, 90] {
        let pdf = dir.path().join(format!("page-{side}.pdf"));
        std::fs::write(&pdf, one_black_page(side)).expect("write the page");
        let png = dir.path().join(format!("page-{side}.png"));
        assert!(
            rasterise(&pdf, &png),
            "sips did not rasterise a well-formed {side} pt page"
        );
        assert_eq!(
            pixel_size(&png),
            Some((side, side)),
            "a {side} pt page became an image of another size"
        );
    }

    // Negative control: the same document cut before its page tree.
    let whole = one_black_page(144);
    let cut = dir.path().join("cut.pdf");
    std::fs::write(&cut, &whole[..whole.len() / 4]).expect("write the cut page");
    let cut_png = dir.path().join("cut.png");
    assert!(
        !rasterise(&cut, &cut_png),
        "sips produced an image from a file cut to a quarter"
    );
}
