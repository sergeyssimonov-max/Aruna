//! ZIP archive traversal and parsing.
//!
//! This module is the reader: it decides which entries are worth inflating,
//! inflates them, and hands the bytes on. What a window *means* belongs to
//! [`crate::parse`], and what order the finished records go in belongs to
//! [`crate::order`] — neither needs an archive to be exercised or measured.

use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use crate::order::sort_records;
use crate::parse::{
    is_manuscript_xml, looks_like_manuscript, parse_manuscript, ManuscriptRecord, HEADER_READ_LIMIT,
};
use crate::progress::Event;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use zip::ZipArchive;

/// Where the time went, for `examples/bench_parse.rs`.
///
/// Collected by the same function that ships rather than by a second one built
/// for measuring: a benchmark that runs a different shape from the program
/// measures the benchmark. What makes that affordable is [`Probe`] — the
/// program runs this pipeline with a probe that has no clock in it at all.
///
/// `#[non_exhaustive]`: stages are what this type is for, and adding one should
/// not be a breaking change.
#[derive(Default, Clone, Copy, Debug)]
#[non_exhaustive]
pub struct StageTimes {
    /// Inflating header windows out of the ZIP.
    pub inflate: Duration,
    /// Turning those windows into records.
    pub parse: Duration,
    /// Putting the finished records in display order.
    ///
    /// Split out from `parse`, which used to carry both. They are separate
    /// costs with separate fixes — one scales with the archive, the other with
    /// the inventory — and added together neither could be seen to move.
    pub sort: Duration,
}

/// The stages [`StageTimes`] keeps apart.
#[derive(Clone, Copy)]
enum Stage {
    Inflate,
    Parse,
    Sort,
}

/// Somewhere to record how long a stage took.
///
/// Two implementations, one pipeline. [`Untimed`] has no clock: `start` returns
/// `None` unconditionally and `record` does nothing, so monomorphising the
/// pipeline against it leaves no timing code in the program at all. That
/// matters here because the inner calls are per entry — the archive has some
/// 24 500 of them, and reading a clock three times each is work the program was
/// doing for a benchmark it was not running.
trait Probe {
    /// The clock now, or `None` when nothing is being measured.
    fn start(&self) -> Option<Instant>;
    /// Charge the time since `since` to `stage`.
    fn record(&mut self, stage: Stage, since: Option<Instant>);
}

/// The probe the program runs with: no clock, no cost.
struct Untimed;

impl Probe for Untimed {
    #[inline(always)]
    fn start(&self) -> Option<Instant> {
        None
    }

    #[inline(always)]
    fn record(&mut self, _stage: Stage, _since: Option<Instant>) {}
}

/// The probe the benchmark runs with.
#[derive(Default)]
struct Timed {
    times: StageTimes,
}

impl Probe for Timed {
    #[inline]
    fn start(&self) -> Option<Instant> {
        Some(Instant::now())
    }

    #[inline]
    fn record(&mut self, stage: Stage, since: Option<Instant>) {
        let Some(since) = since else { return };
        let elapsed = since.elapsed();
        match stage {
            Stage::Inflate => self.times.inflate += elapsed,
            Stage::Parse => self.times.parse += elapsed,
            Stage::Sort => self.times.sort += elapsed,
        }
    }
}

/// Open `zip_path`, parse every manuscript XML, return records ordered for display.
pub fn parse_zip(zip_path: &Path, job: &Job<'_>) -> Result<Vec<ManuscriptRecord>> {
    read_archive(zip_path, &mut Untimed, job)
}

/// As [`parse_zip`], and how long each stage took.
pub fn parse_zip_timed(
    zip_path: &Path,
    job: &Job<'_>,
) -> Result<(Vec<ManuscriptRecord>, StageTimes)> {
    let mut probe = Timed::default();
    let records = read_archive(zip_path, &mut probe, job)?;
    Ok((records, probe.times))
}

/// Buffers the entry loop refills rather than rebuilds.
///
/// One archive is 24 500 entries, and each pass used to allocate the entry's
/// name, the inflated bytes, and a `String` to hold them — three allocations
/// per entry for data discarded before the next one is read. Kept together so
/// it is obvious they live across iterations on purpose.
#[derive(Default)]
struct Scratch {
    /// The current entry's name, copied out before the entry is consumed.
    path: String,
    /// The inflated window, at most [`HEADER_READ_LIMIT`] bytes.
    raw: Vec<u8>,
    /// Only used when `raw` is not valid UTF-8; see [`push_utf8_lossy`].
    repaired: String,
}

/// The pipeline, once, with whatever probe the caller brought.
fn read_archive<P: Probe>(
    zip_path: &Path,
    probe: &mut P,
    job: &Job<'_>,
) -> Result<Vec<ManuscriptRecord>> {
    let mut archive = open_archive(zip_path)?;

    let mut records = Vec::new();
    let mut skipped = Skipped::default();
    let mut scratch = Scratch {
        raw: Vec::with_capacity(HEADER_READ_LIMIT),
        ..Scratch::default()
    };

    for i in 0..archive.len() {
        // Between entries, which is where stopping leaves something coherent:
        // nothing has been written, and the records built so far are simply
        // dropped. One relaxed atomic load against inflating a ZIP entry is not
        // a cost worth measuring, and it is what makes a 24 500-entry archive
        // interruptible at all.
        job.check(Phase::Parsing)?;

        let started = probe.start();
        let entry = archive.by_index(i)?;

        // Copied out because reading the entry consumes it, and into the buffer
        // from the last pass because 24 500 names is 24 500 allocations.
        scratch.path.clear();
        scratch.path.push_str(entry.name());

        // The path gate, which costs nothing: an entry rejected here is never
        // inflated.
        if !is_manuscript_xml(&scratch.path) {
            skipped.by_path(&scratch.path);
            probe.record(Stage::Inflate, started);
            continue;
        }

        read_header_window(entry, &scratch.path, &mut scratch.raw)?;

        // Lossy is correct here rather than lenient: cutting the entry at
        // HEADER_READ_LIMIT routinely splits a multi-byte character, so invalid
        // UTF-8 at the tail is expected, not a corrupt archive. The ordinary
        // case is a window that is already valid, and that one is read where it
        // lies — copying 16 KiB per manuscript to prove it was fine was the
        // single largest copy in the loop.
        let xml = match std::str::from_utf8(&scratch.raw) {
            Ok(text) => text,
            Err(_) => {
                scratch.repaired.clear();
                push_utf8_lossy(&mut scratch.repaired, &scratch.raw);
                scratch.repaired.as_str()
            }
        };
        // Charged here rather than before the conversion: making the bytes
        // readable is part of getting the window out of the archive, and timing
        // it on the other side of the line would report a cheaper read stage by
        // moving work out of it rather than out of the program.
        probe.record(Stage::Inflate, started);

        // The content gate. A path can only be checked against junk that is
        // already known; this asks whether the bytes are a manuscript, which is
        // what a new release of the archive will be judged by.
        if !looks_like_manuscript(xml) {
            skipped.by_content += 1;
            continue;
        }

        let started = probe.start();
        records.push(parse_manuscript(&scratch.path, xml));
        probe.record(Stage::Parse, started);
    }
    skipped.report(job);

    if records.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }

    let started = probe.start();
    sort_records(&mut records);
    probe.record(Stage::Sort, started);

    Ok(records)
}

/// The most entries an archive may declare.
///
/// TLHdig Beta 0.3 is 24 537 entries, of which 23 936 are manuscripts; the rest
/// are the AppleDouble stubs macOS put in the zip. Twenty times that is a
/// ceiling no edition of this corpus approaches and no honest growth of it will.
///
/// It bounds the one cost the per-document limit does not. Every entry is a
/// name copied out of the central directory and a decision about it, before
/// anything is inflated — so an archive of a million empty entries is cheap to
/// build, passes every size check there is, and spends this program's time and
/// memory a name at a time. The central directory declares the count, so this
/// is checked before the first entry is touched.
pub const MAX_ENTRIES: usize = 500_000;

/// Open the ZIP, refusing one with absurdly many entries in it.
pub(crate) fn open_zip(zip_path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    open_zip_within(zip_path, MAX_ENTRIES)
}

/// The limit as an argument, so the boundary can be tested with three entries
/// rather than five hundred thousand.
fn open_zip_within(zip_path: &Path, limit: usize) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(zip_path).map_err(|source| ArunaError::Io {
        path: zip_path.to_path_buf(),
        source,
    })?;
    let archive = ZipArchive::new(BufReader::with_capacity(256 * 1024, file))?;
    if archive.len() > limit {
        return Err(ArunaError::ArchiveTooManyEntries {
            entries: archive.len(),
            limit,
        });
    }
    Ok(archive)
}

/// Open the ZIP for the inventory pass, which also has nothing to do with an
/// empty one.
///
/// The export makes its own decision about emptiness — it reports it as "no
/// manuscripts found" after the gates, which is a different sentence — so the
/// shared half is [`open_zip`] and this is the half that is not shared.
fn open_archive(zip_path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let archive = open_zip(zip_path)?;
    if archive.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }
    Ok(archive)
}

/// Inflate at most [`HEADER_READ_LIMIT`] bytes of one entry into `into`.
fn read_header_window(entry: impl Read, path: &str, into: &mut Vec<u8>) -> Result<()> {
    into.clear();
    entry
        .take(HEADER_READ_LIMIT as u64)
        .read_to_end(into)
        .map_err(|source| ArunaError::Io {
            path: PathBuf::from(path),
            source,
        })?;
    Ok(())
}

/// Append `bytes` to `out` as UTF-8, replacing what is not, into a buffer that
/// already exists.
///
/// `String::from_utf8_lossy` would allocate a second string to hand back, which
/// is what this loop is here to avoid; the replacement it produces is the same,
/// character for character, and `lossy_repair_matches_the_standard_library`
/// holds it to that.
fn push_utf8_lossy(out: &mut String, mut bytes: &[u8]) {
    const REPLACEMENT: char = '\u{FFFD}';

    loop {
        match std::str::from_utf8(bytes) {
            Ok(valid) => {
                out.push_str(valid);
                return;
            }
            Err(error) => {
                let good = error.valid_up_to();
                // Valid by definition of `valid_up_to`; the fallback is
                // unreachable and costs nothing to spell out.
                out.push_str(std::str::from_utf8(&bytes[..good]).unwrap_or(""));
                out.push(REPLACEMENT);
                match error.error_len() {
                    Some(bad) => bytes = &bytes[good + bad..],
                    // An incomplete sequence at the very end: one replacement
                    // for the tail, and nothing left to read.
                    None => return,
                }
            }
        }
    }
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
        // Compared in place. Lowercasing the name first allocated a `String`
        // for every entry in the archive to look at its last four bytes.
        let bytes = path.as_bytes();
        let is_xml = bytes
            .len()
            .checked_sub(4)
            .is_some_and(|from| bytes[from..].eq_ignore_ascii_case(b".xml"));
        if is_xml {
            self.by_path += 1;
        }
    }

    /// Reported rather than silent: the archive is republished from time to
    /// time, and its debris changes with it. A run that suddenly discards
    /// thousands of entries should say so while there is still someone reading
    /// the output.
    fn report(&self, job: &Job<'_>) {
        if self.by_path + self.by_content > 0 {
            job.report(Event::EntriesSkipped {
                by_path: self.by_path,
                by_content: self.by_content,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    /// The entry count is refused before an entry is touched, and the boundary
    /// is where a ceiling is either right or off by one.
    ///
    /// Three entries against a limit of three and of two: the archive that is
    /// exactly at the ceiling opens, the one a single entry past it does not.
    /// Writing five hundred thousand entries to prove the same arithmetic would
    /// cost seconds on every run of this suite and prove nothing extra — which
    /// is why the limit is an argument here and a constant at the call site.
    #[test]
    fn an_archive_with_more_entries_than_the_ceiling_is_refused_unread() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("many.zip");
        write_zip(
            &path,
            &[
                ("CTH 1_XML/a.xml", "<AOxml/>"),
                ("CTH 1_XML/b.xml", "<AOxml/>"),
                ("CTH 1_XML/c.xml", "<AOxml/>"),
            ],
        );

        assert!(
            open_zip_within(&path, 3).is_ok(),
            "an archive exactly at the ceiling is not over it"
        );
        match open_zip_within(&path, 2) {
            Err(ArunaError::ArchiveTooManyEntries { entries, limit }) => {
                assert_eq!((entries, limit), (3, 2));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn write_zip(path: &Path, files: &[(&str, &str)]) {
        let f = File::create(path).unwrap();
        let mut zw = ZipWriter::new(f);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in files {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }

    /// The loop reads a valid window where it lies and only repairs an invalid
    /// one, so the repair has to be the same repair the standard library makes
    /// — including where it puts a replacement character and how many.
    #[test]
    fn lossy_repair_matches_the_standard_library() {
        let cuneiform = "𒀀𒀁𒀂".as_bytes();
        let cases: Vec<Vec<u8>> = vec![
            b"".to_vec(),
            b"plain ascii".to_vec(),
            "ünïcode".as_bytes().to_vec(),
            // A window cut mid-character, which is what the read limit does.
            cuneiform[..cuneiform.len() - 1].to_vec(),
            cuneiform[..cuneiform.len() - 2].to_vec(),
            cuneiform[..1].to_vec(),
            // Rubbish in the middle rather than at the end.
            b"before \xFF\xFE after".to_vec(),
            b"\x80".to_vec(),
            b"\xC0\x80".to_vec(),
            vec![0xED, 0xA0, 0x80], // a surrogate, which is not UTF-8
            b"a\xE2\x82".to_vec(),
            b"\xF0\x9F\x98".to_vec(),
        ];

        for bytes in cases {
            let mut out = String::from("kept: ");
            push_utf8_lossy(&mut out, &bytes);
            assert_eq!(
                out,
                format!("kept: {}", String::from_utf8_lossy(&bytes)),
                "repair differs for {bytes:?}"
            );
        }
    }

    /// The name gate counts XML it turned away and stays quiet about the rest,
    /// which it now decides without lowercasing the name first.
    #[test]
    fn only_entries_claiming_to_be_xml_are_counted_as_skipped() {
        let mut skipped = Skipped::default();
        for path in [
            "a/b.xml", "a/b.XML", "a/b.XmL", // counted
            "a/b.txt", "a/b.xmlx", "xml", ".xml", "a/bxml", "",
        ] {
            skipped.by_path(path);
        }
        // The three spellings of `.xml`, plus `.xml` on its own, which is a
        // four-byte name that ends with it.
        assert_eq!(skipped.by_path, 4);
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
        let recs = parse_zip(&zip_path, &Job::unattended()).unwrap();
        assert_eq!(recs.len(), 2);
        assert!(recs[0].title.contains("A") || recs[1].title.contains("A"));
    }

    #[test]
    fn empty_zip_errors() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("empty.zip");
        write_zip(&zip_path, &[]);
        let err = parse_zip(&zip_path, &Job::unattended()).unwrap_err();
        assert!(matches!(err, ArunaError::EmptyArchive | ArunaError::Zip(_)));
    }

    #[test]
    fn zip_without_xml_errors() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("noxml.zip");
        write_zip(&zip_path, &[("notes.txt", "hi")]);
        assert!(matches!(
            parse_zip(&zip_path, &Job::unattended()).unwrap_err(),
            ArunaError::EmptyArchive
        ));
    }

    #[test]
    fn corrupted_zip_errors() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("bad.zip");
        std::fs::write(&zip_path, b"not a zip at all").unwrap();
        assert!(matches!(
            parse_zip(&zip_path, &Job::unattended()).unwrap_err(),
            ArunaError::Zip(_)
        ));
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
        let recs = parse_zip(&zip_path, &Job::unattended()).unwrap();
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
                (
                    "CTH 1_XML/tiny.xml",
                    "<AOHeader><docID>T</docID></AOHeader>",
                ),
                ("CTH 2_XML/big.xml", &large),
            ],
        );
        let recs = parse_zip(&zip_path, &Job::unattended()).unwrap();
        assert_eq!(recs.len(), 2);
    }
}
