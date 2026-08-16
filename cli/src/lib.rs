//! Aruna — TLHdig (Zenodo) HTML inventory generator.
//!
//! Library surface used by the CLI binary and integration tests.

pub mod archive;
pub mod cache;
pub mod catalog;
pub mod xml_scan;
pub mod download;
pub mod error;
pub mod html;
pub mod md5;
pub mod parse;
pub mod paths;

use error::{ArunaError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Human-readable Zenodo source attribution.
///
/// This is the line the generated HTML prints and the string that travels into
/// `inventory.json` and from there into the ARUN binary the site loads — so it
/// is what every reader is told the data came from.
///
/// It names the record and the edition, not the file: the ZIP's name told a
/// reader nothing they could act on — the record number is what identifies the
/// data and what a citation needs. `source_label_names_the_record_it_downloads`
/// keeps it and [`download::ZENODO_ZIP_URL`] from drifting apart when the
/// archive is republished.
pub const SOURCE_LABEL: &str = "Zenodo record 20328284 — TLHdig Beta 0.3";

/// Full pipeline: download → parse → write HTML → return output path.
///
/// When `local_zip` is `Some`, the download step is skipped (tests / offline).
pub fn run(local_zip: Option<&Path>) -> Result<PathBuf> {
    // Asked before anything expensive: the inventory is written last, and a
    // destination that will refuse it refuses it just as well now.
    let out = paths::output_html_path()?;
    paths::check_output_writable(&out)?;

    let source = match local_zip {
        Some(p) => cache::Archive::Cached(p.to_path_buf()),
        None => obtain_archive()?,
    };

    eprintln!("Parsing XML manuscripts…");
    let records = archive::parse_zip(source.path())?;
    eprintln!("Indexed {} manuscripts.", records.len());

    let generated_at = format_now_local();
    let html = html::render_html(&records, SOURCE_LABEL, &generated_at);

    // Atomic: a failure here must not destroy the inventory an earlier run left
    // in place — see `paths::write_atomic`.
    paths::write_atomic(&out, html.as_bytes())?;

    if let cache::Archive::Temporary(path) = &source {
        // Nowhere to cache it, so this copy was only ever for this run. A failed
        // run keeps it on purpose: the partial state is worth inspecting.
        if let Some(dir) = path.parent() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    Ok(out)
}

/// The archive: from the cache when it is there, otherwise downloaded.
///
/// A hit costs the 267 ms of rereading 71 MiB to check the digest; a miss costs
/// the download, which is about a minute. That is the whole reason this exists.
fn obtain_archive() -> Result<cache::Archive> {
    let url = download::ZENODO_ZIP_URL;
    let md5 = download::ZENODO_ZIP_MD5;

    let Some(dir) = cache::cache_dir() else {
        // No cache directory on this platform: download into a scratch
        // directory this run owns, and delete it at the end as before.
        let work_dir = work_dir_for_process();
        fs::create_dir_all(&work_dir).map_err(|source| ArunaError::Io {
            path: work_dir.clone(),
            source,
        })?;
        let dest = work_dir.join(cache::archive_name(url, md5));
        eprintln!("Downloading TLHdig archive from Zenodo…");
        download::download_verified(url, &dest, Some(md5))?;
        return Ok(cache::Archive::Temporary(dest));
    };

    // Leftovers from runs that were killed mid-download; see `sweep_unfinished`.
    cache::sweep_unfinished(&dir);

    if let Some(hit) = cache::lookup(&dir, url, md5) {
        eprintln!("Using the archive already downloaded: {}", hit.display());
        return Ok(cache::Archive::Cached(hit));
    }

    fs::create_dir_all(&dir).map_err(|source| ArunaError::Io {
        path: dir.clone(),
        source,
    })?;
    let dest = dir.join(cache::archive_name(url, md5));
    eprintln!("Downloading TLHdig archive from Zenodo…");
    // The download lands through a scratch file and a rename, so an interrupted
    // run cannot leave half an archive under a name that promises a whole one.
    download::download_verified(url, &dest, Some(md5))?;
    eprintln!("Kept for the next run: {}", dest.display());
    cache::prune(&dir, &dest);
    Ok(cache::Archive::Cached(dest))
}

/// Scratch directory for this process.
///
/// The path carries the process id: with a fixed name two concurrent runs
/// downloaded into the same file and each read the other's half-written bytes.
fn work_dir_for_process() -> PathBuf {
    std::env::temp_dir().join(format!("aruna-work.{}", std::process::id()))
}

fn format_now_local() -> String {
    // Avoid chrono dependency: format UTC unix-derived timestamp compactly.
    // For inventory metadata, UTC is acceptable and deterministic.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Manual YYYY-MM-DD HH:MM:SS UTC from unix seconds (civil algorithm).
    let (y, m, d, hh, mm, ss) = civil_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

/// Howard Hinnant civil-from-days algorithm (UTC).
fn civil_from_unix(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    let ss = (secs % 60) as u32;
    let mins = secs / 60;
    let mm = (mins % 60) as u32;
    let hours = mins / 60;
    let hh = (hours % 24) as u32;
    let days = (hours / 24) as i64;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, hh, mm, ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two runs must never share a scratch directory — that is the whole point
    /// of putting the process id in the name.
    #[test]
    fn work_dir_is_process_scoped() {
        let dir = work_dir_for_process();
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("work dir has a name");
        assert_eq!(name, format!("aruna-work.{}", std::process::id()));
        assert_eq!(dir.parent(), Some(std::env::temp_dir().as_path()));
        assert_eq!(dir, work_dir_for_process(), "must be stable within a run");
    }

    /// The attribution the reader sees must name the record the tool fetches.
    ///
    /// `SOURCE_LABEL` spells out the record number a second time, in prose, and
    /// nothing so far connected the two: republishing the archive means editing
    /// `ZENODO_ZIP_URL` and `ZENODO_ZIP_MD5` — the digest mismatch makes
    /// forgetting the second one loud — while a stale label goes on quietly
    /// crediting the old record on the page and in the catalog the site ships.
    /// Derive the number from the URL so that edit cannot be half-finished.
    ///
    /// The label no longer repeats the file name, so there is nothing else here
    /// to check: a reader cites the record, and the ZIP's name was noise in a
    /// line that has to fit on one.
    #[test]
    fn source_label_names_the_record_it_downloads() {
        let url = download::ZENODO_ZIP_URL;
        let record = url
            .split("/records/")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .expect("the Zenodo URL names a record");

        assert!(
            SOURCE_LABEL.contains(&format!("record {record}")),
            "SOURCE_LABEL credits a different record than {url} — it reads {SOURCE_LABEL:?}"
        );
    }

    #[test]
    fn civil_epoch() {
        let (y, m, d, hh, mm, ss) = civil_from_unix(0);
        assert_eq!((y, m, d, hh, mm, ss), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn civil_known_date() {
        // 2026-08-10 00:00:00 UTC
        // unix for 2026-01-01 00:00:00 = 1767225600 (approx) — compute via known
        // 2020-01-01 00:00:00 UTC = 1577836800
        let (y, m, d, ..) = civil_from_unix(1_577_836_800);
        assert_eq!((y, m, d), (2020, 1, 1));
    }
}
