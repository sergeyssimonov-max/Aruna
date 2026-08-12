//! Download the TLHdig ZIP from Zenodo.

use crate::error::{ArunaError, Result};
use crate::md5::Md5;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Stable Zenodo URL for TLHdig Beta 0.3.
pub const ZENODO_ZIP_URL: &str =
    "https://zenodo.org/records/20328284/files/TLHbasisONLINE25_1_ZENODO_Beta_03.zip?download=1";

/// MD5 published by Zenodo for record 20328284, verified against the fixture.
///
/// If Zenodo ever republishes the archive this has to be updated together with
/// [`ZENODO_ZIP_URL`]; the mismatch error prints both digests, so the diagnosis
/// is immediate.
pub const ZENODO_ZIP_MD5: &str = "f9acbc8db3111cc7dd88d82f7819a912";

/// Attempts per download before giving up.
const MAX_ATTEMPTS: u32 = 3;

/// Download `url` into `dest`, retrying transient failures.
///
/// No integrity check — see [`download_verified`] for that.
pub fn download_file(url: &str, dest: &Path) -> Result<()> {
    download_verified(url, dest, None)
}

/// Download `url` into `dest`, retrying transient failures and rejecting the
/// result unless it hashes to `expected_md5`.
///
/// Retries cover the failures a second attempt can actually fix: a dropped
/// connection, a short body, a wrong digest, a local write error. An HTTP status
/// is never retried — a 404 stays a 404, and re-requesting it only hammers the
/// archive.
pub fn download_verified(url: &str, dest: &Path, expected_md5: Option<&str>) -> Result<()> {
    let mut attempt = 1;
    loop {
        match attempt_download(url, dest, expected_md5) {
            Ok(()) => return Ok(()),
            Err(err) if attempt < MAX_ATTEMPTS && is_retryable(&err) => {
                eprintln!("Attempt {attempt} failed ({err}); retrying…");
                // Linear backoff: these are transient hiccups, not an overloaded
                // server that needs exponential backoff to recover.
                std::thread::sleep(Duration::from_secs(2 * u64::from(attempt)));
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Whether another attempt has any chance of succeeding.
fn is_retryable(err: &ArunaError) -> bool {
    matches!(
        err,
        ArunaError::Network { .. }
            | ArunaError::Truncated { .. }
            | ArunaError::ChecksumMismatch { .. }
            | ArunaError::Io { .. }
    )
}

/// One transfer: stream to a scratch file, verify, rename into place.
fn attempt_download(url: &str, dest: &Path, expected_md5: Option<&str>) -> Result<()> {
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
    // Hashed as it streams: a second pass over 71 MiB just to digest the file
    // would cost more than the download check saves.
    let mut digest = Md5::new();
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
        digest.update(&buf[..n]);
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

    // Checked before the rename, so a corrupted body never reaches `dest`.
    if let Some(expected) = expected_md5 {
        let got_md5 = digest.finish_hex();
        if !got_md5.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&scratch);
            return Err(ArunaError::ChecksumMismatch {
                url: url.to_string(),
                expected: expected.to_string(),
                got: got_md5,
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
    use crate::md5::md5_hex;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    /// A one-shot HTTP server that serves `bodies[i]` to request `i`, counting
    /// the requests it saw. Local so the retry tests neither touch the network
    /// nor wait on real timeouts.
    struct FakeServer {
        port: u16,
        hits: Arc<AtomicU32>,
    }

    impl FakeServer {
        /// `bodies` are served in order; the last one repeats once exhausted.
        fn start(bodies: Vec<Option<Vec<u8>>>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let hits = Arc::new(AtomicU32::new(0));
            let counter = Arc::clone(&hits);

            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let i = counter.fetch_add(1, Ordering::SeqCst) as usize;
                    let body = bodies
                        .get(i)
                        .or_else(|| bodies.last())
                        .cloned()
                        .flatten();

                    let mut stream = stream;
                    // Read the request line so the client is not writing into a
                    // closed socket while we answer.
                    let mut head = String::new();
                    let mut reader = std::io::BufReader::new(
                        stream.try_clone().expect("clone"),
                    );
                    while reader.read_line(&mut head).unwrap_or(0) > 2 {
                        head.clear();
                    }

                    match body {
                        // `None` means: announce a length, then hang up early —
                        // exactly the truncated-body case.
                        None => {
                            let _ = stream.write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\nshort",
                            );
                        }
                        Some(bytes) => {
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                                bytes.len()
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(&bytes);
                        }
                    }
                    let _ = stream.flush();
                }
            });

            FakeServer { port, hits }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}/archive.zip", self.port)
        }

        fn hits(&self) -> u32 {
            self.hits.load(Ordering::SeqCst)
        }
    }

    /// A body cut short must be rejected, not written out as a complete file.
    ///
    /// Either detector may fire first: `ureq` enforces `Content-Length` while
    /// reading and reports a closed body as an I/O error, and our own count
    /// catches the case where the reader ends cleanly instead. Both are
    /// retryable and both must leave `dest` alone — that is what matters here.
    #[test]
    fn truncated_body_is_rejected() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let server = FakeServer::start(vec![None]);

        let err = download_file(&server.url(), &dest).unwrap_err();
        assert!(
            matches!(err, ArunaError::Truncated { .. } | ArunaError::Io { .. }),
            "unexpected: {err}"
        );
        assert!(is_retryable(&err), "a short body deserves another attempt");
        assert!(!dest.exists());
    }

    /// A transient failure is retried, and the good body that follows wins.
    #[test]
    fn transient_failure_is_retried() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let good = b"complete archive bytes".to_vec();
        let server = FakeServer::start(vec![None, Some(good.clone())]);

        download_verified(&server.url(), &dest, Some(&md5_hex(&good))).expect("second attempt");
        assert_eq!(std::fs::read(&dest).expect("read back"), good);
        assert_eq!(server.hits(), 2, "should have taken exactly two attempts");
    }

    /// A wrong digest is retried too — then reported rather than accepted.
    #[test]
    fn checksum_mismatch_is_reported_after_retries() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let server = FakeServer::start(vec![Some(b"corrupted".to_vec())]);

        let err = download_verified(&server.url(), &dest, Some(&md5_hex(b"expected")))
            .unwrap_err();
        match err {
            ArunaError::ChecksumMismatch { expected, got, .. } => {
                assert_eq!(expected, md5_hex(b"expected"));
                assert_eq!(got, md5_hex(b"corrupted"));
            }
            other => panic!("unexpected: {other}"),
        }
        assert!(!dest.exists(), "corrupt body must not reach the destination");
        assert_eq!(server.hits(), MAX_ATTEMPTS, "every attempt should be used");
    }

    /// A matching digest passes the file through untouched.
    #[test]
    fn matching_checksum_is_accepted() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let body = b"the real archive".to_vec();
        let server = FakeServer::start(vec![Some(body.clone())]);

        download_verified(&server.url(), &dest, Some(&md5_hex(&body))).expect("accepted");
        assert_eq!(std::fs::read(&dest).expect("read back"), body);
        assert_eq!(server.hits(), 1, "no retry needed on success");
    }

    /// An HTTP status is a verdict, not a hiccup: retrying a 404 only hammers
    /// the archive.
    #[test]
    fn http_status_is_not_retried() {
        assert!(!is_retryable(&ArunaError::Http {
            url: "u".into(),
            status: 404,
        }));
        assert!(is_retryable(&ArunaError::Truncated {
            url: "u".into(),
            expected: 2,
            got: 1,
        }));
    }

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
