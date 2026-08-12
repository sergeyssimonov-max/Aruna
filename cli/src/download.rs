//! Download the TLHdig ZIP from Zenodo.

use crate::error::{ArunaError, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Stable Zenodo URL for TLHdig Beta 0.3.
pub const ZENODO_ZIP_URL: &str =
    "https://zenodo.org/records/20328284/files/TLHbasisONLINE25_1_ZENODO_Beta_03.zip?download=1";

/// Download `url` into `dest`, streaming to disk.
///
/// On HTTP failure or transport error returns [`ArunaError`].
pub fn download_file(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ArunaError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(300))
        .user_agent("Aruna/1.0 (+https://github.com/sergeyssimonov-max/Aruna)")
        .build();

    let response = agent.get(url).call().map_err(|e| ArunaError::Network {
        url: url.to_string(),
        source: Box::new(e),
    })?;

    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(ArunaError::Http {
            url: url.to_string(),
            status,
        });
    }

    // Announced size, when the server sends one — used below to catch a body
    // cut short by a dropped connection.
    let expected: Option<u64> = response
        .header("Content-Length")
        .and_then(|v| v.trim().parse().ok());

    let mut reader = response.into_reader();

    // Stream into a scratch file and rename only on success: an interrupted
    // download must never leave a truncated archive sitting at `dest` looking
    // like a complete one.
    let scratch = scratch_path(dest);
    let mut file = File::create(&scratch).map_err(|source| ArunaError::Io {
        path: scratch.clone(),
        source,
    })?;

    let mut got: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    let outcome = loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            Err(source) => break Err(source),
        };
        if let Err(source) = file.write_all(&buf[..n]) {
            break Err(source);
        }
        got += n as u64;
    };
    let outcome = outcome.and_then(|()| file.sync_all());
    drop(file);

    if let Err(source) = outcome {
        let _ = std::fs::remove_file(&scratch);
        return Err(ArunaError::Io {
            path: scratch,
            source,
        });
    }

    if let Some(expected) = expected {
        if got != expected {
            let _ = std::fs::remove_file(&scratch);
            return Err(ArunaError::Truncated {
                url: url.to_string(),
                expected,
                got,
            });
        }
    }

    std::fs::rename(&scratch, dest).map_err(|source| {
        let _ = std::fs::remove_file(&scratch);
        ArunaError::Io {
            path: dest.to_path_buf(),
            source,
        }
    })
}

/// Sibling scratch path for the in-flight download.
///
/// Same filesystem as `dest`, so the closing rename is atomic; the process id
/// keeps concurrent runs from sharing one scratch file.
fn scratch_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.part", std::process::id()));
    dest.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn network_error_on_unreachable_host() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let err = download_file("http://127.0.0.1:1/nope.zip", &dest).unwrap_err();
        match err {
            ArunaError::Network { .. } | ArunaError::Http { .. } => {}
            other => panic!("unexpected error variant: {other}"),
        }
    }

    /// A failed transfer must leave the destination untouched and no scratch
    /// file behind — a truncated ZIP at `dest` would parse as a corrupt archive
    /// on the next run instead of being re-downloaded.
    #[test]
    fn failed_download_leaves_no_files_behind() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        assert!(download_file("http://127.0.0.1:1/nope.zip", &dest).is_err());
        assert!(!dest.exists(), "destination must not be created on failure");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name())
            .collect();
        assert!(leftovers.is_empty(), "scratch left behind: {leftovers:?}");
    }

    /// An existing archive stays intact when a later download fails.
    #[test]
    fn failed_download_preserves_previous_file() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        std::fs::write(&dest, b"previous good archive").expect("seed");
        assert!(download_file("http://127.0.0.1:1/nope.zip", &dest).is_err());
        assert_eq!(
            std::fs::read(&dest).expect("read back"),
            b"previous good archive"
        );
    }

    #[test]
    fn http_error_on_404() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        // httpbin may be flaky; use zenodo missing file for a real 404
        let err = download_file(
            "https://zenodo.org/records/20328284/files/this-file-does-not-exist-aruna.zip",
            &dest,
        )
        .unwrap_err();
        match err {
            ArunaError::Http { status, .. } => assert!(status == 404 || status == 403),
            ArunaError::Network { .. } => {
                // offline CI environments are acceptable
            }
            other => panic!("unexpected: {other}"),
        }
    }
}
