//! The PDF items of the release gate (specification 6.8, layer 2; owner's
//! decisions of 2026-09-30, questions 6, 7 and 12). Not a converter and not
//! run in the pre-commit set: the whole corpus, on the owner's machine.
//!
//! ```sh
//! cd cli
//! cargo build --release --locked --example pdf_gate
//! G=../target/release/examples/pdf_gate
//! $G build  <archive.zip> <fonts> <destination> [off]  # one build, its time and sum
//! $G series <archive.zip> <fonts> <scratch> 5          # five builds, five processes
//! $G check  <package> <fonts> [--c14n] [--pdfkit]      # every criterion, every PDF
//! $G sum    <package>                                  # the package's sum, as 6.7 takes it
//! ```
//!
//! `series` is the time guard: the PDF phase of five builds of the whole set,
//! each its own process, the median against 125 s (4.10) – and the five
//! packages must be one package, byte for byte. `check` reads a built package
//! the way `PDF-ACCEPTANCE.md` asks: nothing outside the margin, no blank
//! glyph, the labels' code points for this project's reader (and PDFKit's with
//! `--pdfkit`), wrappers only where a cluster repeats, the credit where the
//! decision puts it, the tag tree equal to the model's – by string, and with
//! `--c14n` by `xmllint --c14n` – with the four negative controls, the text in
//! logical order equal to the source and the stream's code points the same.

#[path = "../../tests/support/pdf_check.rs"]
mod pdf_check;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use aruna::document::Document;
use aruna::export::{self, Pdf};
use aruna::job::{Cancel, Job};
use aruna::pdf::layout::Page;
use aruna::pdf::Fonts;
use aruna::progress::{Event, Progress};
use pdf_check::*;

/// The product's limit on the PDF phase of the whole set, in seconds (4.10).
const LIMIT: f64 = 125.0;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str).unwrap_or_default();
    let ok = match arg(0) {
        "build" => build(
            Path::new(arg(1)),
            Path::new(arg(2)),
            Path::new(arg(3)),
            arg(4) == "off",
        ),
        "series" => series(
            Path::new(arg(1)),
            Path::new(arg(2)),
            Path::new(arg(3)),
            arg(4).parse().unwrap_or(5),
        ),
        "check" => check_package(
            Path::new(arg(1)),
            Path::new(arg(2)),
            args.iter().any(|a| a == "--c14n"),
            args.iter().any(|a| a == "--pdfkit"),
        ),
        "sum" => {
            let (files, sum) = package_sum(Path::new(arg(1)));
            println!("files {files}\nall {sum}");
            true
        }
        _ => {
            eprintln!("usage: pdf_gate build|series|check|sum …");
            false
        }
    };
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------------------
// The build, timed
// ---------------------------------------------------------------------------

/// Times the PDF phase by its own events: from `WritingPdfs` to the last tick.
struct Clock {
    started: std::sync::Mutex<Option<Instant>>,
    phase: std::sync::Mutex<Option<f64>>,
}

impl Progress for Clock {
    fn report(&self, event: Event<'_>) {
        match event {
            Event::WritingPdfs { .. } => {
                *self.started.lock().expect("unpoisoned") = Some(Instant::now());
            }
            Event::PdfsWritten { done, total, .. } if done == total => {
                if let Some(t) = *self.started.lock().expect("unpoisoned") {
                    *self.phase.lock().expect("unpoisoned") = Some(t.elapsed().as_secs_f64());
                }
            }
            _ => {}
        }
    }
}

fn build(zip: &Path, fonts: &Path, destination: &Path, off: bool) -> bool {
    let fonts = Fonts::load(fonts).expect("the fonts of docs/FONTS.md");
    std::fs::create_dir_all(destination).expect("destination");
    let clock = Clock {
        started: std::sync::Mutex::new(None),
        phase: std::sync::Mutex::new(None),
    };
    let cancel = Cancel::new();
    let started = Instant::now();
    let pdf = if off { Pdf::Off } else { Pdf::On(&fonts) };
    let built = export::build_with(
        zip,
        destination,
        aruna::SOURCE_LABEL,
        pdf,
        &Job::new(&clock, &cancel),
    );
    let whole = started.elapsed().as_secs_f64();
    let built = match built {
        Ok(b) => b,
        Err(e) => {
            println!("FAILED {e}");
            return false;
        }
    };
    let phase = clock.phase.lock().expect("unpoisoned").unwrap_or(0.0);
    let (files, sum) = package_sum(&destination.join(export::PACKAGE));
    println!("phase_seconds {phase:.1}");
    println!("build_seconds {whole:.1}");
    println!("documents {}", built.documents);
    if let Some(p) = built.pdfs {
        println!("pdfs built {} refused {}", p.built, p.refused);
    }
    println!("package {sum} {files}");
    true
}

/// The sum of 6.7: every file, sorted by its path's bytes (`LC_ALL=C`),
/// `shasum -a 256` of each, `shasum -a 256` of those lines.
fn package_sum(root: &Path) -> (usize, String) {
    let mut paths = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readable").flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                paths.push(p);
            }
        }
    }
    let mut rel: Vec<(Vec<u8>, PathBuf)> = paths
        .into_iter()
        .map(|p| {
            let r = format!(
                "./{}",
                p.strip_prefix(root).expect("inside").to_string_lossy()
            );
            (r.into_bytes(), p)
        })
        .collect();
    rel.sort();
    let mut lines = String::new();
    for (r, p) in &rel {
        let digest = aruna::sha256::sha256_file(p).expect("readable");
        lines.push_str(&format!("{digest}  {}\n", String::from_utf8_lossy(r)));
    }
    (rel.len(), aruna::sha256::sha256_hex(lines.as_bytes()))
}

fn series(zip: &Path, fonts: &Path, scratch: &Path, n: usize) -> bool {
    let me = std::env::current_exe().expect("the gate itself");
    let mut times = Vec::new();
    let mut sums = Vec::new();
    for i in 1..=n {
        let dest = scratch.join(format!("run-{i}"));
        let _ = std::fs::remove_dir_all(&dest);
        let out = Command::new(&me)
            .args(["build"])
            .arg(zip)
            .arg(fonts)
            .arg(&dest)
            .output()
            .expect("a build process");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        print!("run {i}:\n{text}");
        let field = |key: &str| {
            text.lines()
                .find_map(|l| l.strip_prefix(key))
                .map(str::trim)
                .map(String::from)
        };
        times.push(
            field("phase_seconds")
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(f64::MAX),
        );
        sums.push(field("package").unwrap_or_default());
        let _ = std::fs::remove_dir_all(&dest);
    }
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    let same = sums.windows(2).all(|w| w[0] == w[1]) && !sums[0].is_empty();
    println!("times {times:?}");
    println!(
        "median {median:.1} s against {LIMIT} s: {}",
        if median <= LIMIT { "within" } else { "OVER" }
    );
    println!(
        "packages of {n} processes: {}",
        if same { "one package" } else { "DIFFER" }
    );
    median <= LIMIT && same
}

// ---------------------------------------------------------------------------
// Every criterion, on every PDF of a package
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Tally {
    documents: usize,
    pdfs: usize,
    refused_by_model: usize,
    missing: Vec<String>,
    outside: Vec<String>,
    trimmed_spaces: usize,
    notdef: Vec<String>,
    credit: Vec<String>,
    credits: usize,
    composition: Vec<String>,
    tree: Vec<String>,
    orphans_or_twice: Vec<String>,
    logical: Vec<String>,
    label_docs: usize,
    labels: usize,
    labels_own: Vec<String>,
    wrapped: Vec<String>,
    wrapper_without_marks: Vec<String>,
    krilla_actual_text: usize,
    c14n: Vec<String>,
    pdfkit: Vec<String>,
    pdfkit_label_docs: usize,
    pdfkit_teeth: Vec<String>,
    /// Whether PDFKit still gives the code point with the ToUnicode gone.
    pdfkit_names: Option<bool>,
    controls: Vec<String>,
}

fn check_package(package: &Path, fonts: &Path, c14n: bool, pdfkit: bool) -> bool {
    let fonts = Fonts::load(fonts).expect("the fonts of docs/FONTS.md");
    let manifest = std::fs::read_to_string(package.join(export::MANIFEST)).expect("a manifest");
    // The documents, from the groups: `file` is a key elsewhere too.
    let groups = &manifest[manifest
        .find("\"groups\": [")
        .expect("the manifest lists groups")..];
    let files = export::manifest::values_of(groups, "file");
    let sigla = export::manifest::values_of(groups, "siglum");
    assert_eq!(files.len(), sigla.len(), "the manifest's documents");
    let trees = std::env::temp_dir().join(format!("pdf_gate-trees-{}", std::process::id()));
    if c14n {
        std::fs::create_dir_all(&trees).expect("scratch");
    }
    let mut t = Tally::default();
    let mut label_pdfs: Vec<(PathBuf, BTreeMap<u32, usize>)> = Vec::new();
    let started = Instant::now();
    for (file, siglum) in files.iter().zip(&sigla) {
        t.documents += 1;
        let xml = package.join(file);
        let pdf_path = xml.with_extension("pdf");
        let bytes = std::fs::read(&xml).expect("the document");
        let Ok(doc) = Document::read(&bytes) else {
            t.refused_by_model += 1;
            if pdf_path.exists() {
                t.missing
                    .push(format!("{file}: refused by the model and has a PDF"));
            }
            continue;
        };
        let Ok(pdf) = std::fs::read(&pdf_path) else {
            t.missing
                .push(format!("{file}: read by the model and has no PDF"));
            continue;
        };
        t.pdfs += 1;
        let group = file.split('/').next().unwrap_or_default();
        let page = Page::of(&doc, group, siglum);
        let c = match check(&pdf) {
            Ok(c) => c,
            Err(e) => {
                t.missing.push(format!("{file}: unreadable: {e}"));
                continue;
            }
        };
        let credit = page.uses_credited_face(&fonts);
        if !c.outside.is_empty() {
            t.outside.push(format!("{file}: {}", c.outside[0]));
        }
        t.trimmed_spaces += c.trimmed_spaces;
        if c.notdef > 0 {
            t.notdef.push(file.clone());
        }
        if !c.credit_ok() || c.ullikummi_embedded != credit {
            t.credit.push(format!("{file}: {:?}", c.credit_pages));
        }
        if credit {
            t.credits += 1;
        }
        if !c.same_composition(&page) {
            t.composition.push(file.clone());
        }
        let want = expected_tree(&fonts, &page, credit);
        let got = export_tree(&pdf).expect("tagged");
        if let Some(d) = tree_difference(&want, &got.xml) {
            t.tree.push(format!("{file}: {d:?}"));
        }
        if got.orphans + got.twice > 0 {
            t.orphans_or_twice.push(file.clone());
        }
        // The credit is the tree's first paragraph where it stands: a known
        // addition to the text (PDF-ACCEPTANCE §7), expected once.
        let logical = if credit {
            format!("{}\n{}", aruna::fonts::CREDIT, expected_text(&page))
        } else {
            expected_text(&page)
        };
        if first_difference(&cps(&logical), &cps(&tree_text(&got.xml))).is_some() {
            t.logical.push(file.clone());
        }
        if c14n {
            let stem = trees.join(t.pdfs.to_string());
            std::fs::write(stem.with_extension("want.xml"), &want).expect("scratch");
            std::fs::write(stem.with_extension("got.xml"), &got.xml).expect("scratch");
            if canonical(&stem.with_extension("want.xml"))
                != canonical(&stem.with_extension("got.xml"))
            {
                t.c14n.push(file.clone());
            }
        }
        let labels = page.labels(&fonts);
        if !labels.is_empty() {
            t.label_docs += 1;
            t.labels += labels.len();
            let mut want: BTreeMap<u32, usize> = BTreeMap::new();
            for cp in &labels {
                *want.entry(*cp).or_insert(0) += 1;
            }
            let own: usize = want
                .iter()
                .map(|(cp, n)| {
                    let found = c.raw.chars().filter(|ch| u32::from(*ch) == *cp).count();
                    usize::from(found != *n)
                })
                .sum();
            if own > 0 || c.label_letters > 0 {
                t.labels_own.push(file.clone());
            }
            label_pdfs.push((pdf_path.clone(), want));
        }
        // Wrappers: an ActualText span at the top of the stream is ours
        // (variant A); krilla's sits inside a marked span.
        let scans = scan_pdf(&pdf).expect("pages");
        let ours = scans
            .iter()
            .flat_map(|s| &s.spans)
            .filter(|s| s.actual_text.is_some() && s.mcid.is_none() && s.parent.is_none())
            .count();
        let krilla = scans
            .iter()
            .flat_map(|s| &s.spans)
            .filter(|s| s.actual_text.is_some() && s.parent.is_some())
            .count();
        if ours > 0 {
            t.wrapped.push(file.clone());
            if !page.has_combining_marks() {
                t.wrapper_without_marks.push(file.clone());
            }
        }
        if krilla > 0 {
            t.krilla_actual_text += 1;
        }
        if t.documents.is_multiple_of(2000) {
            eprintln!(
                "  {} documents, {:.0} s",
                t.documents,
                started.elapsed().as_secs_f64()
            );
        }
    }
    if pdfkit {
        run_pdfkit(&label_pdfs, &mut t);
    }
    controls(package, &fonts, &files, &sigla, &mut t);
    if c14n {
        let _ = std::fs::remove_dir_all(&trees);
    }
    report(&t, c14n, pdfkit)
}

fn canonical(path: &Path) -> Vec<u8> {
    Command::new("xmllint")
        .arg("--c14n")
        .arg(path)
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default()
}

/// Whether one line of the PDFKit reader gives every label's code point as
/// many times as the page sets it, and no label letters.
fn pdfkit_agrees(line: &str, want: &BTreeMap<u32, usize>) -> bool {
    let fields: Vec<&str> = line.trim_end().split('\t').collect();
    if fields.first() != Some(&"OPENED") {
        return false;
    }
    let found: BTreeMap<u32, usize> = fields
        .get(3)
        .unwrap_or(&"")
        .split(',')
        .filter_map(|p| {
            let (cp, n) = p.split_once(':')?;
            Some((u32::from_str_radix(cp, 16).ok()?, n.parse().ok()?))
        })
        .collect();
    let letters: usize = fields.get(4).and_then(|n| n.parse().ok()).unwrap_or(1);
    letters == 0 && want.iter().all(|(cp, n)| found.get(cp) == Some(n))
}

fn run_pdfkit(label_pdfs: &[(PathBuf, BTreeMap<u32, usize>)], t: &mut Tally) {
    let bin = std::env::temp_dir().join(format!("pdf_gate-pdfkit-{}", std::process::id()));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pdf_gate/pdfkit.swift");
    let built = Command::new("swiftc")
        .args(["-O", "-framework", "PDFKit"])
        .arg(&source)
        .arg("-o")
        .arg(&bin)
        .status()
        .is_ok_and(|s| s.success());
    if !built {
        t.pdfkit
            .push("swiftc did not build the reader: the property is UNMET, not passed".into());
        return;
    }
    // The reader's teeth first: a spoiled label, and a layer without its
    // ToUnicode and its glyph names, on the first document with labels must
    // both be caught, or the reading below proves nothing.
    if let Some((path, want)) = label_pdfs.first() {
        let pdf = std::fs::read(path).expect("the PDF");
        for (what, spoiled) in [
            ("spoiled label", spoil_label(&pdf)),
            ("lost ToUnicode and glyph names", drop_label_names(&pdf)),
        ] {
            let probe = bin.with_extension(format!("{}.pdf", what.replace(' ', "-")));
            let caught = spoiled.is_ok_and(|bytes| {
                std::fs::write(&probe, bytes).expect("scratch");
                let out = Command::new(&bin)
                    .arg(&probe)
                    .output()
                    .expect("the reader runs");
                !pdfkit_agrees(&String::from_utf8_lossy(&out.stdout), want)
            });
            let _ = std::fs::remove_file(&probe);
            t.pdfkit_teeth.push(format!(
                "{what}: {}",
                if caught { "caught" } else { "MISSED" }
            ));
        }
        // ToUnicode alone: PDFKit falls back on the glyph names, as the layer
        // means it to. Said, not counted as a tooth.
        let probe = bin.with_extension("lost-ToUnicode.pdf");
        if let Ok(bytes) = drop_label_to_unicode(&pdf) {
            std::fs::write(&probe, bytes).expect("scratch");
            let out = Command::new(&bin)
                .arg(&probe)
                .output()
                .expect("the reader runs");
            t.pdfkit_names = Some(pdfkit_agrees(&String::from_utf8_lossy(&out.stdout), want));
            let _ = std::fs::remove_file(&probe);
        }
    }
    for chunk in label_pdfs.chunks(200) {
        let out = Command::new(&bin)
            .args(chunk.iter().map(|(p, _)| p))
            .output()
            .expect("the reader runs");
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        for ((path, want), line) in chunk.iter().zip(text.lines()) {
            if line.starts_with("OPENED") {
                t.pdfkit_label_docs += 1;
            }
            if !pdfkit_agrees(line, want) {
                t.pdfkit
                    .push(format!("{}: want {want:?}, PDFKit {line}", path.display()));
            }
        }
    }
    let _ = std::fs::remove_file(&bin);
}

/// The four negative controls on the documents the trial used: each spoils
/// the file one way, and each must break the equality of the trees.
fn controls(package: &Path, fonts: &Fonts, files: &[String], sigla: &[String], t: &mut Tally) {
    for (name, wanted) in [("KUB 5.1+", "abc"), ("KBo 52.182", "acd")] {
        let Some(i) = sigla.iter().position(|s| s == name) else {
            t.controls.push(format!("{name}: not in the package"));
            continue;
        };
        let bytes = std::fs::read(package.join(&files[i])).expect("the document");
        let doc = Document::read(&bytes).expect("a document of the model");
        let group = files[i].split('/').next().unwrap_or_default();
        let page = Page::of(&doc, group, name);
        let credit = page.uses_credited_face(fonts);
        let want = expected_tree(fonts, &page, credit);
        let tree_of = |p: &Page| {
            let r =
                aruna::pdf::render_page(p, "control", fonts, aruna::pdf::TEMPLATE).expect("builds");
            export_tree(&r.pdf).expect("tagged").xml
        };
        let equal = tree_of(&page) == want;
        let mut line = format!(
            "{name}: positive {}",
            if equal { "equal" } else { "DIFFERS" }
        );
        for control in wanted.chars() {
            let spoiled = match control {
                'a' => swap_lines(&page).map(|p| tree_of(&p)),
                'b' => drop_note(&page).map(|p| tree_of(&p)),
                'c' => repeat_line(&page).map(|p| tree_of(&p)),
                _ => {
                    let pdf = std::fs::read(package.join(&files[i]).with_extension("pdf"))
                        .expect("the PDF");
                    spoil_label(&pdf)
                        .ok()
                        .and_then(|p| export_tree(&p).ok())
                        .map(|e| e.xml)
                }
            };
            let caught = spoiled.is_some_and(|s| s != want);
            line.push_str(&format!(
                ", ({control}) {}",
                if caught { "caught" } else { "MISSED" }
            ));
        }
        t.controls.push(line);
    }
}

fn report(t: &Tally, c14n: bool, pdfkit: bool) -> bool {
    let row = |name: &str, bad: &[String]| {
        println!(
            "| {name} | {} |",
            if bad.is_empty() {
                "0".to_string()
            } else {
                format!("{} – {}", bad.len(), bad[0])
            }
        );
        bad.is_empty()
    };
    println!(
        "documents {}, PDFs {}, refused by the model {}",
        t.documents, t.pdfs, t.refused_by_model
    );
    println!("| criterion | on the whole set |\n|---|---|");
    let mut ok = true;
    ok &= row(
        "a document without its PDF, or a PDF without its document",
        &t.missing,
    );
    ok &= row("outside the margin", &t.outside);
    println!(
        "| trimmed end-of-line spaces past the edge, not counted | {} |",
        t.trimmed_spaces
    );
    ok &= row("blank glyphs (code 0 in TJ)", &t.notdef);
    ok &= row("credit not where the decision puts it", &t.credit);
    println!("| files with the credit | {} |", t.credits);
    ok &= row("stream code points not the source's", &t.composition);
    ok &= row("tag tree not the model's (string)", &t.tree);
    ok &= row("MCID outside the tree or twice in it", &t.orphans_or_twice);
    ok &= row(
        "text of the tree not the source in logical order",
        &t.logical,
    );
    if c14n {
        ok &= row("tag tree not the model's (xmllint --c14n)", &t.c14n);
    }
    println!(
        "| documents with labels / labels | {} / {} |",
        t.label_docs, t.labels
    );
    ok &= row(
        "labels: code point missing or letters extracted, own reader",
        &t.labels_own,
    );
    if pdfkit {
        println!(
            "| label documents PDFKit opened | {} |",
            t.pdfkit_label_docs
        );
        ok &= row("labels at PDFKit", &t.pdfkit);
        for line in &t.pdfkit_teeth {
            println!("| PDFKit teeth | {line} |");
            ok &= line.ends_with("caught");
        }
        ok &= t.pdfkit_teeth.len() == 2;
        if let Some(kept) = t.pdfkit_names {
            println!(
                "| PDFKit, ToUnicode alone removed | {} |",
                if kept {
                    "code point kept, from the glyph names"
                } else {
                    "code point lost"
                }
            );
        }
    }
    println!(
        "| documents with our wrappers (variant A) | {} |",
        t.wrapped.len()
    );
    ok &= row(
        "our wrapper in a document without a combining mark",
        &t.wrapper_without_marks,
    );
    println!(
        "| documents with krilla's own ActualText (known property) | {} |",
        t.krilla_actual_text
    );
    for line in &t.controls {
        println!("| control | {line} |");
        ok &= !line.contains("DIFFERS") && !line.contains("MISSED") && !line.contains("not in");
    }
    println!(
        "verdict: {}",
        if ok {
            "every criterion holds"
        } else {
            "NOT MET"
        }
    );
    ok
}
