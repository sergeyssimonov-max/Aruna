//! Refresh the snapshot of CTH titles from the catalogue.
//!
//! ```sh
//! cargo run --locked --release -p aruna --example cth_titles            # writes resources/cth/cth-titles.tsv
//! cargo run --locked --release -p aruna --example cth_titles -- page.html # a saved page, no network;
//!                                                                           # `fetched` is the file's mtime
//! ```
//!
//! This is the only thing in the repository that fetches the catalogue, and it
//! is not part of building a package or of starting the application: both read
//! the snapshot compiled into the crate. The new snapshot is written beside the
//! old one under a temporary name and renamed over it only once the page has
//! parsed; a failed fetch or parse leaves the old snapshot exactly as it was.
#![forbid(unsafe_code)]

use aruna::cth_titles::{parse_catalog_page, render_snapshot, Entry, SOURCE_URL};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

/// A page larger than this is not the catalogue.
const PAGE_LIMIT: u64 = 8 * 1024 * 1024;

fn main() -> ExitCode {
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/cth/cth-titles.tsv");
    match run(&target, std::env::args().nth(1).map(PathBuf::from)) {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!(
                "cth_titles: {e}; the snapshot at {} is unchanged",
                target.display()
            );
            ExitCode::FAILURE
        }
    }
}

fn run(target: &Path, saved_page: Option<PathBuf>) -> Result<String, String> {
    // When the page was obtained: now for a fetch, the file's modification
    // time for a page saved earlier — never the moment of parsing.
    let (page, fetched) = match &saved_page {
        Some(path) => {
            let page = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let written = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .ok_or_else(|| format!("{} has no modification time", path.display()))?;
            (page, aruna::cth_titles::utc_from_unix(written.as_secs()))
        }
        None => (fetch()?, aruna::cth_titles::now_utc()),
    };
    let sha = aruna::sha256::sha256_hex(&page);
    let html = String::from_utf8(page).map_err(|_| "the page is not UTF-8".to_string())?;
    let entries = parse_catalog_page(&html)?;
    let text = render_snapshot(&entries, &fetched, &sha);
    aruna::cth_titles::Catalog::parse(&text).map_err(|e| {
        format!(
            "the new snapshot does not read back: line {} {}",
            e.line, e.reason
        )
    })?;

    let tmp = target.with_extension(format!("tsv.tmp-{}", std::process::id()));
    std::fs::write(&tmp, &text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("replace {}: {e}", target.display()));
    }
    let unassigned = entries
        .iter()
        .filter(|(_, e)| *e == Entry::Unassigned)
        .count();
    Ok(format!(
        "{}: {} entries, {} titled, {} unassigned, page sha256 {sha}",
        target.display(),
        entries.len(),
        entries.len() - unassigned,
        unassigned
    ))
}

fn fetch() -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout(Duration::from_secs(120))
        .build();
    let response = agent
        .get(SOURCE_URL)
        .call()
        .map_err(|e| format!("fetch {SOURCE_URL}: {e}"))?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(PAGE_LIMIT + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("read {SOURCE_URL}: {e}"))?;
    if body.len() as u64 > PAGE_LIMIT {
        return Err(format!("the page is larger than {PAGE_LIMIT} bytes"));
    }
    Ok(body)
}
