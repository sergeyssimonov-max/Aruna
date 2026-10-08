//! The walk and the two patches on files written by hand: every operator the
//! walk knows, every font form it reads, and every way a patch can find a file
//! it cannot place its text in. Typst writes only part of what ISO 32000
//! allows; the checks of the gate read with this same walk, so the rest is
//! held here rather than trusted.

use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use super::*;

/// A document of one page, the content and the resources given; the id of
/// the page.
fn one_page(content: &[u8], resources: Object) -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");
    let contents = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
        "Contents" => contents,
        "Resources" => resources,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page.into()],
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    (doc, page)
}

fn to_unicode(doc: &mut Document, cmap: &str) -> ObjectId {
    doc.add_object(Stream::new(dictionary! {}, cmap.as_bytes().to_vec()))
}

fn ok<T>(r: Result<T, PdfError>) -> T {
    r.unwrap_or_else(|e| panic!("{e}"))
}

fn saved(doc: &mut Document) -> Vec<u8> {
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap_or_else(|e| panic!("{e}"));
    out
}

/// A composite font with both forms of `W`, a simple font with `Widths`, a
/// Type3 font with its own matrix; ToUnicode by `bfchar` and `bfrange`.
#[test]
fn the_walk_reads_every_font_form_and_text_operator() {
    let (mut doc, page) = one_page(b"", Object::Null);
    let cmap2 = to_unicode(
        &mut doc,
        "1 beginbfchar\n<0001> <0041>\nendbfchar\n1 beginbfrange\n<0002> <0004> <0061>\nendbfrange\n",
    );
    let cid = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "DW" => 500,
        "W" => vec![1.into(), vec![600.into(), 700.into()].into(), 3.into(), 4.into(), 800.into()],
    });
    let type0 = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "ABCDEF+Test",
        "DescendantFonts" => vec![cid.into()], "ToUnicode" => cmap2,
    });
    let cmap1 = to_unicode(&mut doc, "1 beginbfchar\n<41> <0078>\nendbfchar\n");
    let simple = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Simple",
        "FirstChar" => 65, "Widths" => vec![Object::Real(500.0)], "ToUnicode" => cmap1,
    });
    let type3 = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type3",
        "FontMatrix" => vec![Object::Real(0.01), 0.into(), 0.into(), Object::Real(0.01), 0.into(), 0.into()],
        "FirstChar" => 1, "Widths" => vec![100.into()],
    });
    let resources =
        dictionary! { "Font" => dictionary! { "F0" => type0, "F1" => simple, "F3" => type3 } };
    let content = b"q 1 0 0 1 10 10 cm 0.5 g BT /F0 10 Tf 12 TL 1 Tc 90 Tz 2 Ts 1 0 0 1 5 50 Tm \
<00010002> Tj T* [<0003> -500 <0004>] TJ 0 -12 Td <0000> Tj 0 -12 TD /F1 10 Tf (A) Tj /F3 10 Tf <01> Tj ET Q \
0 0 m 10 10 l 1 2 3 4 5 6 c 20 20 5 5 re S";
    let stream = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
    let page_dict = ok(doc.get_dictionary_mut(page).map_err(broken));
    page_dict.set("Contents", stream);
    page_dict.set("Resources", resources);
    let scan = ok(scan_page(&doc, page));
    assert_eq!((scan.width, scan.height), (200.0, 100.0));
    assert_eq!(scan.text, "Aabcx");
    assert_eq!(scan.raw_text, "Aabcx");
    assert_eq!(
        scan.notdef.get("ABCDEF+Test"),
        Some(&1),
        "{:?}",
        scan.notdef
    );
    assert_eq!(scan.glyphs.len(), 7);
    assert_eq!(scan.path_points.len(), 7);
    assert!(scan.glyphs[0].x0 > 10.0 && scan.glyphs[0].x1 > scan.glyphs[0].x0);
    let mut fonts = scan.fonts.clone();
    fonts.sort();
    assert_eq!(fonts, ["", "ABCDEF+Test", "Simple"]);
}

/// Marked content: an ActualText span gives its text to the MCID span around
/// it, and the glyphs it replaces are not read twice.
#[test]
fn the_walk_resolves_actual_text_into_the_spans_it_replaces() {
    let (mut doc, page) = one_page(b"", Object::Null);
    let cmap = to_unicode(
        &mut doc,
        "2 beginbfchar\n<41> <0041>\n<42> <0301>\nendbfchar\n",
    );
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "F", "ToUnicode" => cmap,
    });
    let content = "BT /F 10 Tf /P <</MCID 0>> BDC /Span <</ActualText (xy)>> BDC /Span <</MCID 1>> BDC (AB) Tj EMC EMC EMC \
/Artifact BMC (A) Tj EMC ET 0 0 m 5 5 l W n";
    let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let d = ok(doc.get_dictionary_mut(page).map_err(broken));
    d.set("Contents", stream);
    d.set(
        "Resources",
        dictionary! { "Font" => dictionary! { "F" => font } },
    );
    let scan = ok(scan_page(&doc, page));
    assert_eq!(scan.text, "xyA");
    assert_eq!(scan.raw_text, "A\u{301}A");
    let by = |m: i64| {
        scan.spans
            .iter()
            .find(|s| s.mcid == Some(m))
            .map(|s| s.resolved.clone())
    };
    assert_eq!(by(0).as_deref(), Some("xy"));
    // Inside an MCID span already, the ActualText is that span's, and the
    // replaced span within reads nothing: the text is counted once.
    assert_eq!(by(1).as_deref(), Some(""));
    assert_eq!(decode_text_string(&[0xFE, 0xFF, 0x00, 0x41]), "A");
}

/// The two-byte ToUnicode and a label on a page: the page's shape as Typst
/// writes it – one content stream, an image's span with an MCID, clipped.
const LABEL_PAGE: &[u8] =
    b"q /Span<</MCID 0>>BDC q 1 0 0 1 20 30 cm 0 0 m 30 0 l 30 10 l h W n Q EMC Q";

fn label(cp: u32) -> Label {
    Label {
        cp,
        height_em: 0.62,
        depth_share: 0.2,
        width_em: 2.0,
    }
}

fn patched(doc: &mut Document, labels: &[Label]) -> Result<(Vec<u8>, Patched), PdfError> {
    finish(&saved(doc), labels, false)
}

#[test]
fn a_label_gets_its_invisible_text_in_every_form_of_resources() {
    // Resources inline, no fonts: the dictionary is made.
    let (mut doc, page) = one_page(LABEL_PAGE, Object::Dictionary(dictionary! {}));
    let (pdf, report) = ok(patched(&mut doc, &[label(0xE83A)]));
    assert_eq!((report.labels, report.unmatched), (1, 0));
    let doc = ok(Document::load_mem(&pdf).map_err(broken));
    let scan = ok(scan_page(&doc, page));
    assert_eq!(scan.text, "\u{E83A}");
    // Resources by reference, fonts by reference.
    let (mut doc, _) = one_page(LABEL_PAGE, Object::Null);
    let fonts = doc.add_object(dictionary! {});
    let resources = doc.add_object(dictionary! { "Font" => fonts });
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    ok(doc.get_dictionary_mut(pages[0]).map_err(broken)).set("Resources", resources);
    let (pdf, _) = ok(patched(&mut doc, &[label(0x100009)]));
    assert!(memchr::memmem::find(&pdf, b"ArunaLabel").is_some());
}

#[test]
fn a_label_that_cannot_be_placed_stops_the_build() {
    let invariant = |r: Result<(Vec<u8>, Patched), PdfError>| match r {
        Err(PdfError::Invariant(i)) => i,
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("placed"),
    };
    // Resources of no known form.
    let (mut doc, _) = one_page(LABEL_PAGE, Object::Integer(1));
    assert!(matches!(
        invariant(patched(&mut doc, &[label(1)])),
        Invariant::Pdf(_)
    ));
    // A font of the same name already there.
    let (mut doc, _) = one_page(
        LABEL_PAGE,
        Object::Dictionary(dictionary! { "Font" => dictionary! { "ArunaLabel" => 1 } }),
    );
    assert!(matches!(
        invariant(patched(&mut doc, &[label(1)])),
        Invariant::Pdf(_)
    ));
    // Two content streams.
    let (mut doc, page) = one_page(LABEL_PAGE, Object::Dictionary(dictionary! {}));
    let second = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
    let first = ok(doc.get_dictionary(page).map_err(broken))
        .get(b"Contents")
        .cloned();
    let first = first.unwrap_or_else(|e| panic!("{e}"));
    ok(doc.get_dictionary_mut(page).map_err(broken)).set("Contents", vec![first, second.into()]);
    assert!(matches!(
        invariant(patched(&mut doc, &[label(1)])),
        Invariant::Pdf(_)
    ));
    // The span written two ways: found by the walk, not by its bytes.
    let (mut doc, _) = one_page(
        b"/Span <</MCID 0>> BDC q 0 0 m 3 0 l 3 3 l h W n Q EMC",
        Object::Dictionary(dictionary! {}),
    );
    assert_eq!(
        invariant(patched(&mut doc, &[label(1)])),
        Invariant::LabelSpans(1)
    );
    // One code point, two widths.
    let (mut doc, _) = one_page(
        b"/Span<</MCID 0>>BDC 0 0 m 3 0 l 3 3 l h W n EMC /Span<</MCID 1>>BDC 0 0 m 3 0 l 3 3 l h W n EMC",
        Object::Dictionary(dictionary! {}),
    );
    let wide = Label {
        width_em: 5.0,
        ..label(7)
    };
    assert!(matches!(
        invariant(patched(&mut doc, &[label(7), wide])),
        Invariant::Pdf(_)
    ));
}

/// Pieces as Typst writes a shifted mark: `A` + U+0301 read by the first
/// piece and again by the next.
fn cluster_page(content: &str) -> Vec<u8> {
    let (mut doc, page) = one_page(b"", Object::Null);
    let cmap = to_unicode(
        &mut doc,
        "3 beginbfchar\n<41> <0041>\n<42> <0301>\n<43> <00410301>\nendbfchar\n",
    );
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "F", "ToUnicode" => cmap,
    });
    let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let d = ok(doc.get_dictionary_mut(page).map_err(broken));
    d.set("Contents", stream);
    d.set(
        "Resources",
        dictionary! { "Font" => dictionary! { "F" => font } },
    );
    saved(&mut doc)
}

#[test]
fn a_repeated_cluster_is_wrapped_and_nothing_else_is() {
    let pdf = cluster_page(
        "BT /F 10 Tf /P<</MCID 0>>BDC (C) Tj EMC /P<</MCID 1>>BDC (B) Tj EMC /P<</MCID 2>>BDC (A) Tj EMC /P<</MCID 3>>BDC (B) Tj EMC ET",
    );
    let (out, report) = ok(finish(&pdf, &[], true));
    assert_eq!(report.clusters.len(), 1, "{:?}", report.clusters);
    let doc = ok(Document::load_mem(&out).map_err(broken));
    let page = doc.get_pages().values().copied().next().unwrap_or((0, 0));
    assert_eq!(ok(scan_page(&doc, page)).text, "A\u{301}A\u{301}");
    // No repeat anywhere: the bytes come back as they went.
    let plain =
        cluster_page("BT /F 10 Tf /P<</MCID 0>>BDC (A) Tj EMC /P<</MCID 1>>BDC (B) Tj EMC ET");
    assert_eq!(ok(finish(&plain, &[], true)).0, plain);
}

/// Two repeats in a row share a piece: the PDF of this document is refused,
/// not wrapped twice over itself. Until the owner's decision of 2026-10-02
/// this stopped the whole build as an invariant; the name of the test is kept.
#[test]
fn two_clusters_over_one_piece_stop_the_build() {
    let pdf = cluster_page(
        "BT /F 10 Tf /P<</MCID 0>>BDC (C) Tj EMC /P<</MCID 1>>BDC (B) Tj EMC /P<</MCID 2>>BDC (B) Tj EMC ET",
    );
    match finish(&pdf, &[], true) {
        Err(e @ PdfError::Clusters { page: 1, mcid: 1 }) => assert!(!e.is_invariant()),
        other => panic!("{:?}", other.map(|(_, r)| r)),
    }
}

/// Two repeats with a piece between them are two wrappers and no refusal.
#[test]
fn two_clusters_apart_are_both_wrapped() {
    let pdf = cluster_page(
        "BT /F 10 Tf /P<</MCID 0>>BDC (C) Tj EMC /P<</MCID 1>>BDC (B) Tj EMC /P<</MCID 2>>BDC (A) Tj EMC /P<</MCID 3>>BDC (C) Tj EMC /P<</MCID 4>>BDC (B) Tj EMC ET",
    );
    let (out, report) = ok(finish(&pdf, &[], true));
    assert_eq!(report.clusters.len(), 2, "{:?}", report.clusters);
    let doc = ok(Document::load_mem(&out).map_err(broken));
    let page = doc.get_pages().values().copied().next().unwrap_or((0, 0));
    assert_eq!(ok(scan_page(&doc, page)).text, "A\u{301}AA\u{301}");
}

#[test]
fn a_cluster_on_a_page_of_two_streams_stops_the_build() {
    let (mut doc, page) = one_page(b"", Object::Null);
    let cmap = to_unicode(
        &mut doc,
        "2 beginbfchar\n<42> <0301>\n<43> <00410301>\nendbfchar\n",
    );
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "F", "ToUnicode" => cmap,
    });
    let a = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F 10 Tf /P<</MCID 0>>BDC (C) Tj EMC".to_vec(),
    ));
    let b = doc.add_object(Stream::new(
        dictionary! {},
        b"/P<</MCID 1>>BDC (B) Tj EMC ET".to_vec(),
    ));
    let d = ok(doc.get_dictionary_mut(page).map_err(broken));
    d.set("Contents", vec![a.into(), b.into()]);
    d.set(
        "Resources",
        dictionary! { "Font" => dictionary! { "F" => font } },
    );
    assert!(matches!(
        finish(&saved(&mut doc), &[], true),
        Err(PdfError::Invariant(Invariant::Pdf(_)))
    ));
}

/// Outside any MCID span, an ActualText span without one gives its text to
/// the first MCID span opened inside it – the form krilla writes.
#[test]
fn an_outer_actual_text_goes_to_the_first_marked_span_inside() {
    let (mut doc, page) = one_page(b"", Object::Null);
    let content = "/Span <</ActualText <FEFF00DC>>> BDC /P <</MCID 4>> BDC EMC EMC";
    let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    ok(doc.get_dictionary_mut(page).map_err(broken)).set("Contents", stream);
    let scan = ok(scan_page(&doc, page));
    let span = scan.spans.iter().find(|s| s.mcid == Some(4));
    assert_eq!(span.map(|s| s.resolved.as_str()), Some("\u{DC}"));
}
