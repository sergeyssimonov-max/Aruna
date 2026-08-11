//! Download the TLHdig ZIP from Zenodo.

use crate::error::{ArunaError, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
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

    let mut reader = response.into_reader();
    let mut file = File::create(dest).map_err(|source| ArunaError::Io {
        path: dest.to_path_buf(),
        source,
    })?;

    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|source| ArunaError::Io {
            path: dest.to_path_buf(),
            source,
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|source| ArunaError::Io {
            path: dest.to_path_buf(),
            source,
        })?;
    }
    file.flush().map_err(|source| ArunaError::Io {
        path: dest.to_path_buf(),
        source,
    })?;

    Ok(())
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
