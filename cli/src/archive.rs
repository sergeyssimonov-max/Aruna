//! ZIP archive traversal and batch parsing.

use crate::error::{ArunaError, Result};
use crate::parse::{is_manuscript_xml, parse_manuscript, ManuscriptRecord, HEADER_READ_LIMIT};
use rayon::prelude::*;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use zip::ZipArchive;

/// Open `zip_path`, parse every manuscript XML, return records grouped by CTH.
///
/// Reads at most [`HEADER_READ_LIMIT`] bytes per entry (AOHeader always fits) and
/// parses with the SIMD heuristic engine. CPU-bound parsing is parallelised with
/// Rayon after sequential ZIP inflation.
pub fn parse_zip(zip_path: &Path) -> Result<Vec<ManuscriptRecord>> {
    let file = File::open(zip_path).map_err(|source| ArunaError::Io {
        path: zip_path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::with_capacity(256 * 1024, file);
    let mut archive = ZipArchive::new(reader)?;

    if archive.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }

    let mut entries: Vec<(String, String)> = Vec::with_capacity(archive.len().min(32_768));

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if !is_manuscript_xml(&name) {
            continue;
        }

        // Only inflate the header window — bodies are huge and unused.
        let mut limited = entry.take(HEADER_READ_LIMIT as u64);
        let mut bytes = Vec::with_capacity(8 * 1024);
        limited
            .read_to_end(&mut bytes)
            .map_err(|source| ArunaError::Io {
                path: Path::new(&name).to_path_buf(),
                source,
            })?;

        let xml = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        };
        entries.push((name, xml));
    }

    if entries.is_empty() {
        return Err(ArunaError::EmptyArchive);
    }

    let records: Vec<ManuscriptRecord> = entries
        .par_iter()
        .map(|(name, xml)| parse_manuscript(name, xml))
        .collect();

    // Group by CTH number, then natural-order sigla (KBo 3.22 before KBo 22.5).
    let mut keyed: Vec<(u32, Vec<NatPart>, ManuscriptRecord)> = records
        .into_par_iter()
        .map(|r| {
            let key = natural_sigla_key(&r.sigla);
            (r.cth_num, key, r)
        })
        .collect();
    keyed.par_sort_unstable_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.authorship.cmp(&b.2.authorship))
            .then_with(|| a.2.year.cmp(&b.2.year))
    });
    let records: Vec<ManuscriptRecord> = keyed.into_iter().map(|(_, _, r)| r).collect();

    Ok(records)
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
