//! ZIP archive traversal and parsing.

use crate::error::{ArunaError, Result};
use crate::parse::{
    is_manuscript_xml, looks_like_manuscript, parse_manuscript, ManuscriptRecord, HEADER_READ_LIMIT,
};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zip::ZipArchive;

/// Where the time went, for `examples/bench_parse.rs`.
///
/// Collected by the same function that ships rather than by a second one built
/// for measuring: a benchmark that runs a different shape from the program
/// measures the benchmark.
#[derive(Default, Clone, Copy)]
pub struct StageTimes {
    /// Inflating header windows out of the ZIP.
    pub inflate: Duration,
    /// Turning those windows into records.
    pub parse: Duration,
}

/// Open `zip_path`, parse every manuscript XML, return records ordered for display.
pub fn parse_zip(zip_path: &Path) -> Result<Vec<ManuscriptRecord>> {
    Ok(parse_zip_timed(zip_path)?.0)
}

/// As [`parse_zip`], and how long each stage took.
///
/// Each entry is inflated, parsed and dropped before the next is read. Holding
/// the whole corpus first — 23 936 windows of up to 16 KiB — cost 240 MiB of
/// peak memory for data that is finished with the moment it becomes a record,
/// and grew with the archive rather than with the inventory.
pub fn parse_zip_timed(zip_path: &Path) -> Result<(Vec<ManuscriptRecord>, StageTimes)> {
    let mut archive = open_archive(zip_path)?;

    let mut records = Vec::new();
    let mut skipped = Skipped::default();
    let mut times = StageTimes::default();

    for i in 0..archive.len() {
        let started = Instant::now();
        let entry = archive.by_index(i)?;
        let path = entry.name().to_string();

        // The path gate, which costs nothing: an entry rejected here is never
        // inflated.
        if !is_manuscript_xml(&path) {
            skipped.by_path(&path);
            times.inflate += started.elapsed();
            continue;
        }

        let xml = read_header_window(entry, &path)?;
        times.inflate += started.elapsed();

        // The content gate. A path can only be checked against junk that is
        // already known; this asks whether the bytes are a manuscript, which is
        // what a new release of the archive will be judged by.
        if !looks_like_manuscript(&xml) {
            skipped.by_content += 1;
            continue;
        }

        let started = Instant::now();
        records.push(parse_manuscript(&path, &xml));
        times.parse += started.elapsed();
    }
    skipped.report();

    if records.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }

    let started = Instant::now();
    sort_records(&mut records);
    times.parse += started.elapsed();

    Ok((records, times))
}

/// Open the ZIP, refusing one with nothing in it.
fn open_archive(zip_path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(zip_path).map_err(|source| ArunaError::Io {
        path: zip_path.to_path_buf(),
        source,
    })?;
    let archive = ZipArchive::new(BufReader::with_capacity(256 * 1024, file))?;
    if archive.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }
    Ok(archive)
}

/// Inflate at most [`HEADER_READ_LIMIT`] bytes of one entry.
fn read_header_window(entry: impl Read, path: &str) -> Result<String> {
    let mut bytes = Vec::new();
    entry
        .take(HEADER_READ_LIMIT as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| ArunaError::Io {
            path: PathBuf::from(path),
            source,
        })?;

    // Lossy conversion is correct here rather than lenient: cutting the entry at
    // HEADER_READ_LIMIT routinely splits a multi-byte character, so invalid
    // UTF-8 at the tail is expected, not a corrupt archive.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Entries the two gates turned away, counted apart because they mean
/// different things: junk the archive has always carried, and a document that
/// is named like a manuscript but is not one.
#[derive(Default)]
struct Skipped {
    by_path: usize,
    by_content: usize,
}

impl Skipped {
    /// Count a path the name gate rejected — but only if it claimed to be XML.
    ///
    /// The archive is mostly directories, images and stylesheets, and reporting
    /// those as "skipped" would bury the number that matters in five figures of
    /// entries nobody expected to be manuscripts in the first place.
    fn by_path(&mut self, path: &str) {
        if path.to_ascii_lowercase().ends_with(".xml") {
            self.by_path += 1;
        }
    }

    /// Reported rather than silent: the archive is republished from time to
    /// time, and its debris changes with it. A run that suddenly discards
    /// thousands of entries should say so while there is still someone reading
    /// the output.
    fn report(&self) {
        let total = self.by_path + self.by_content;
        if total > 0 {
            eprintln!(
                "Skipped {total} non-manuscript entries ({} by path, {} by content).",
                self.by_path, self.by_content
            );
        }
    }
}

/// Order records for display: by CTH number, then natural-order sigla
/// (`KBo 3.22` before `KBo 22.5`), then editor and year.
///
/// Sequential, like the parsing above. A thread pool was tried on the parse and
/// removed at 1.07×; inflating the ZIP is ~80 % of the run and a single
/// `ZipArchive` reader cannot be read in parallel, so even free parsing and
/// sorting would buy about 1.2× — under the 1.5–2× a dependency has to earn
/// here. See `PERFORMANCE.md`.
pub fn sort_records(records: &mut Vec<ManuscriptRecord>) {
    // Sigla keys are built once per record rather than on every comparison.
    let mut keyed: Vec<(u32, Vec<NatPart>, ManuscriptRecord)> = std::mem::take(records)
        .into_iter()
        .map(|r| (r.cth_num, natural_sigla_key(&r.sigla), r))
        .collect();

    keyed.sort_unstable_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.authorship.cmp(&b.2.authorship))
            .then_with(|| a.2.year.cmp(&b.2.year))
    });

    *records = keyed.into_iter().map(|(_, _, r)| r).collect();
}


/// One segment of a natural-order key for publication sigla.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NatPart {
    Num(u64),
    Text(String),
}

/// Split `KBo 3.22+` → [Text("kbo "), Num(3), Text("."), Num(22), Text("+")] so
/// numeric runs compare as integers (`3.22` before `22.5`).
fn natural_sigla_key(s: &str) -> Vec<NatPart> {
    let lower = s.to_ascii_lowercase();
    let b = lower.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let n: u64 = lower[start..i].parse().unwrap_or(0);
            out.push(NatPart::Num(n));
        } else {
            let start = i;
            while i < b.len() && !b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(NatPart::Text(lower[start..i].to_string()));
        }
    }
    out
}

#[cfg(test)]
mod natural_sort_tests {
    use super::*;

    #[test]
    fn numeric_runs_compare_as_integers() {
        let a = natural_sigla_key("KBo 3.22");
        let b = natural_sigla_key("KBo 22.5");
        assert!(a < b, "expected KBo 3.22 < KBo 22.5, got {a:?} vs {b:?}");
        assert!(natural_sigla_key("KBo 7.30") < natural_sigla_key("KBo 12.18"));
        assert!(natural_sigla_key("KUB 2.1") < natural_sigla_key("KUB 26.71"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_zip(path: &Path, files: &[(&str, &str)]) {
        let f = File::create(path).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, body) in files {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }

    #[test]
    fn parses_multiple_entries() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        write_zip(
            &zip_path,
            &[
                (
                    "CTH 1_XML/A.xml",
                    r#"<AOHeader><docID>A</docID><uebern editor="AA" date="2018-01-01"/></AOHeader>"#,
                ),
                (
                    "CTH 2_XML/B.xml",
                    r#"<AOHeader><docID>B</docID><uebern editor="BB" date="2019-02-02"/></AOHeader>"#,
                ),
                ("readme.txt", "ignore me"),
            ],
        );
        let recs = parse_zip(&zip_path).unwrap();
        assert_eq!(recs.len(), 2);
        assert!(recs[0].title.contains("A") || recs[1].title.contains("A"));
    }

    #[test]
    fn empty_zip_errors() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("empty.zip");
        write_zip(&zip_path, &[]);
        let err = parse_zip(&zip_path).unwrap_err();
        assert!(matches!(err, ArunaError::EmptyArchive | ArunaError::Zip(_)));
    }

    #[test]
    fn zip_without_xml_errors() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("noxml.zip");
        write_zip(&zip_path, &[("notes.txt", "hi")]);
        assert!(matches!(
            parse_zip(&zip_path).unwrap_err(),
            ArunaError::EmptyArchive
        ));
    }

    #[test]
    fn corrupted_zip_errors() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("bad.zip");
        std::fs::write(&zip_path, b"not a zip at all").unwrap();
        assert!(matches!(parse_zip(&zip_path).unwrap_err(), ArunaError::Zip(_)));
    }

    #[test]
    fn unicode_paths_in_zip() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("u.zip");
        write_zip(
            &zip_path,
            &[(
                "CTH 222_XML_TLH/İK 174-66.xml",
                r#"<AOHeader><docID>İK 174-66</docID><creation-date date="2023-07-26"/></AOHeader>"#,
            )],
        );
        let recs = parse_zip(&zip_path).unwrap();
        assert_eq!(recs.len(), 1);
        assert!(recs[0].title.contains("İK 174-66"));
    }

    #[test]
    fn tiny_and_large_xml_entries() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("sizes.zip");
        let mut large = String::from(
            r#"<AOHeader><docID>BIG</docID><uebern editor="ZZ" date="2020-01-01"/></AOHeader><body>"#,
        );
        large.push_str(&"𒀀".repeat(50_000));
        large.push_str("</body>");
        write_zip(
            &zip_path,
            &[
                ("CTH 1_XML/tiny.xml", "<AOHeader><docID>T</docID></AOHeader>"),
                ("CTH 2_XML/big.xml", &large),
            ],
        );
        let recs = parse_zip(&zip_path).unwrap();
        assert_eq!(recs.len(), 2);
    }
}
