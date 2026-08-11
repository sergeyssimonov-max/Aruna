//! Aruna — TLHdig (Zenodo) HTML inventory generator.
//!
//! Library surface used by the CLI binary and integration tests.

pub mod archive;
pub mod xml_scan;
pub mod download;
pub mod error;
pub mod html;
pub mod parse;
pub mod paths;

use error::{ArunaError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Human-readable Zenodo source attribution.
pub const SOURCE_LABEL: &str =
    "Zenodo record 20328284 — TLHdig Beta 0.3 (TLHbasisONLINE25_1_ZENODO_Beta_03.zip)";

/// Full pipeline: download → parse → write HTML → return output path.
///
/// When `local_zip` is `Some`, the download step is skipped (tests / offline).
pub fn run(local_zip: Option<&Path>) -> Result<PathBuf> {
    let work_dir = std::env::temp_dir().join("aruna-work");
    fs::create_dir_all(&work_dir).map_err(|source| ArunaError::Io {
        path: work_dir.clone(),
        source,
    })?;

    let zip_path = match local_zip {
        Some(p) => p.to_path_buf(),
        None => {
            let dest = work_dir.join("TLHbasisONLINE25_1_ZENODO_Beta_03.zip");
            eprintln!("Downloading TLHdig archive from Zenodo…");
            download::download_file(download::ZENODO_ZIP_URL, &dest)?;
            dest
        }
    };

    eprintln!("Parsing XML manuscripts…");
    let records = archive::parse_zip(&zip_path)?;
    eprintln!("Indexed {} manuscripts.", records.len());

    let generated_at = format_now_local();
    let html = html::render_html(&records, SOURCE_LABEL, &generated_at);

    let out = paths::output_html_path()?;
    paths::ensure_output_parent(&out)?;
    fs::write(&out, html.as_bytes()).map_err(|source| ArunaError::Io {
        path: out.clone(),
        source,
    })?;

    Ok(out)
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

    #[test]
    fn civil_epoch() {
        let (y, m, d, hh, mm, ss) = civil_from_unix(0);
        assert_eq!((y, m, d, hh, mm, ss), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn civil_known_date() {
        // 2020-01-01 00:00:00 UTC = 1577836800
        let (y, m, d, ..) = civil_from_unix(1_577_836_800);
        assert_eq!((y, m, d), (2020, 1, 1));
    }
}
