//! Prove that normalising a document changes nothing but what it is allowed to.
//!
//! ```text
//! cargo run --release --example verify_normalization -- fixtures/…zip
//! ```
//!
//! The permit list, and nothing else may differ:
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
//! be identical, byte for byte. Not "equivalent", not "the same after
//! normalising both sides" — identical. Comparing normalised forms would hide
//! exactly the corruption this is looking for, and the corpus mixes Unicode
//! forms, so it would hide a lot of it.
//!
//! Checked for every document, not a sample. A sample is diagnosis; this is the
//! claim.

use aruna::export::normalize_into;
use aruna::export::verify;
use aruna::parse::{is_manuscript_xml, looks_like_manuscript, HEADER_READ_LIMIT};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(zip) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: verify_normalization <archive.zip>");
        return ExitCode::FAILURE;
    };

    let before = match aruna::md5::md5_file(&zip) {
        Ok(digest) => digest,
        Err(err) => {
            eprintln!("cannot read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    let file = match std::fs::File::open(&zip) {
        Ok(file) => file,
        Err(err) => {
            eprintln!("cannot open {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };
    let mut archive = match zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file)) {
        Ok(archive) => archive,
        Err(err) => {
            eprintln!("cannot read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    let mut checked = 0usize;
    let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
    let mut added = 0usize;
    let mut reflowed = 0usize;
    let mut violations: Vec<String> = Vec::new();
    let mut source = Vec::new();
    let mut normalised = Vec::new();

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(entry) => entry,
            Err(err) => {
                violations.push(format!("entry {i} could not be read: {err}"));
                continue;
            }
        };
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }
        source.clear();
        if let Err(err) = entry.read_to_end(&mut source) {
            violations.push(format!("{name}: {err}"));
            continue;
        }
        let head = String::from_utf8_lossy(&source[..source.len().min(HEADER_READ_LIMIT)]);
        if !looks_like_manuscript(&head) {
            continue;
        }

        normalised.clear();
        normalize_into(&source, &mut normalised);
        checked += 1;

        match verify::compare(&source, &normalised) {
            Ok(report) => {
                for target in report.dropped {
                    *dropped.entry(target).or_default() += 1;
                }
                if report.added_declaration {
                    added += 1;
                }
                if report.reflowed {
                    reflowed += 1;
                }
            }
            Err(reason) => violations.push(format!("{name}: {reason}")),
        }
    }

    let after = match aruna::md5::md5_file(&zip) {
        Ok(digest) => digest,
        Err(err) => {
            eprintln!("cannot re-read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    println!();
    println!("archive:            {}", zip.display());
    println!("documents checked:  {checked}");
    println!("source digest:      {before}");
    println!(
        "  after the run:    {after}   {}",
        if before == after {
            "unchanged"
        } else {
            "CHANGED — the source was written to"
        }
    );
    println!();
    println!("changes, by permit-list rule:");
    for (target, count) in &dropped {
        println!("  DROP_PI   {target:<16} {count}");
    }
    println!("  ADD       declaration      {added}");
    println!("  REFLOW    prologue space   {reflowed}");
    println!();

    if before != after {
        eprintln!("FAILED: the archive changed while being read");
        return ExitCode::FAILURE;
    }
    if violations.is_empty() {
        println!("no distortion: every byte after the prologue is identical in all {checked}");
        ExitCode::SUCCESS
    } else {
        eprintln!("FAILED: {} document(s) distorted", violations.len());
        for line in violations.iter().take(20) {
            eprintln!("  {line}");
        }
        ExitCode::FAILURE
    }
}
