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

/// Longest we will wait on a server's `Retry-After` before giving up on it.
///
/// Zenodo can answer a 429 with a delay measured in minutes. Sleeping that long
/// inside a run the user is watching is worse than telling them to come back.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

/// Download `url` into `dest`, retrying transient failures and rejecting the
/// result unless it hashes to `expected_md5`.
///
/// Retries cover the failures a second attempt can actually fix: a dropped
/// connection, a short body, a local write error, and the HTTP statuses that
/// mean "busy, not wrong". See [`is_retryable`] for what is deliberately left
/// out.
pub fn download_verified(url: &str, dest: &Path, expected_md5: Option<&str>) -> Result<()> {
    let mut attempt = 1;
    loop {
        match attempt_download(url, dest, expected_md5) {
            Ok(()) => return Ok(()),
            Err(err) if attempt < MAX_ATTEMPTS && is_retryable(&err) => {
                let delay = retry_delay(attempt, &err);
                eprintln!(
                    "Attempt {attempt} failed ({err}); retrying in {}s…",
                    delay.as_secs()
                );
                std::thread::sleep(delay);
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

/// HTTP statuses worth another attempt.
///
/// These say the server could not serve the request *right now*: overloaded,
/// rate-limiting, a bad gateway in front of it. Every other status is a verdict
/// on the request itself, and repeating it only hammers the archive.
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

/// Whether another attempt has any chance of succeeding.
fn is_retryable(err: &ArunaError) -> bool {
    match err {
        ArunaError::Network { .. } | ArunaError::Truncated { .. } | ArunaError::Io { .. } => true,
        ArunaError::Http { status, .. } => is_retryable_status(*status),
        // A digest mismatch is not a hiccup. Either ZENODO_ZIP_MD5 is stale or
        // the archive was republished, and both are settled before the first
        // byte arrives — so the retries downloaded 71 MiB twice more only to
        // reach the identical error. The message names both digests; that is
        // the useful outcome, and it should arrive at once.
        _ => false,
    }
}

/// How long to wait before the next attempt.
///
/// A server that sent `Retry-After` has told us what it wants; anything else
/// gets exponential backoff, which now matters because an overloaded server is
/// among the things we retry.
fn retry_delay(attempt: u32, err: &ArunaError) -> Duration {
    if let ArunaError::Http {
        retry_after: Some(secs),
        ..
    } = err
    {
        return Duration::from_secs(*secs).min(MAX_RETRY_AFTER);
    }
    Duration::from_secs(2u64.saturating_pow(attempt))
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

    // ureq hands back every non-2xx as `Error::Status`, so the status has to be
    // pulled out of the error rather than off a response. Mapping the whole
    // error to `Network` — as this did — buried the status: a 404 was reported
    // as a network failure and, because network failures are retried, fetched
    // three times before the user heard about it.
    let response = match agent.get(url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => {
            return Err(ArunaError::Http {
                url: url.to_string(),
                status,
                // Only the delta-seconds form is read. `Retry-After` may also
                // carry an HTTP-date, but parsing dates to shave a few seconds
                // off a backoff is not worth a date parser.
                retry_after: response
                    .header("Retry-After")
                    .and_then(|v| v.trim().parse().ok()),
            });
        }
        Err(source) => {
            return Err(ArunaError::Network {
                url: url.to_string(),
                source: Box::new(source),
            })
        }
    };

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

    /// What the fake server should do for one request.
    enum Reply {
        /// Serve these bytes with a matching `Content-Length`.
        Body(Vec<u8>),
        /// Announce a length, then hang up early — the truncated-body case.
        Truncated,
        /// Answer with this status, optionally carrying `Retry-After`.
        Status(u16, Option<u64>),
    }

    /// A one-shot HTTP server that serves `replies[i]` to request `i`, counting
    /// the requests it saw. Local so the retry tests neither touch the network
    /// nor wait on real timeouts.
    struct FakeServer {
        port: u16,
        hits: Arc<AtomicU32>,
    }

    impl FakeServer {
        /// Convenience for the common "serve these bodies" case.
        fn with_bodies(bodies: Vec<Option<Vec<u8>>>) -> Self {
            Self::start(
                bodies
                    .into_iter()
                    .map(|b| match b {
                        Some(bytes) => Reply::Body(bytes),
                        None => Reply::Truncated,
                    })
                    .collect(),
            )
        }

        /// `replies` are served in order; the last one repeats once exhausted.
        fn start(replies: Vec<Reply>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let hits = Arc::new(AtomicU32::new(0));
            let counter = Arc::clone(&hits);

            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let i = counter.fetch_add(1, Ordering::SeqCst) as usize;
                    let reply = replies.get(i).or_else(|| replies.last());

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

                    match reply {
                        None | Some(Reply::Truncated) => {
                            let _ = stream.write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\nshort",
                            );
                        }
                        Some(Reply::Body(bytes)) => {
                            let head = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                                bytes.len()
                            );
                            let _ = stream.write_all(head.as_bytes());
                            let _ = stream.write_all(bytes);
                        }
                        Some(Reply::Status(status, retry_after)) => {
                            let mut head = format!(
                                "HTTP/1.1 {status} Something\r\nContent-Length: 0\r\n"
                            );
                            if let Some(secs) = retry_after {
                                head.push_str(&format!("Retry-After: {secs}\r\n"));
                            }
                            head.push_str("\r\n");
                            let _ = stream.write_all(head.as_bytes());
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
        let server = FakeServer::with_bodies(vec![None]);

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
        let server = FakeServer::with_bodies(vec![None, Some(good.clone())]);

        download_verified(&server.url(), &dest, Some(&md5_hex(&good))).expect("second attempt");
        assert_eq!(std::fs::read(&dest).expect("read back"), good);
        assert_eq!(server.hits(), 2, "should have taken exactly two attempts");
    }

    /// A wrong digest is reported at once, on the first download.
    ///
    /// It used to be retried, which meant a stale `ZENODO_ZIP_MD5` pulled 71 MiB
    /// three times to arrive at the identical error.
    #[test]
    fn checksum_mismatch_is_reported_without_downloading_again() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let server = FakeServer::with_bodies(vec![Some(b"corrupted".to_vec())]);

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
        assert_eq!(
            server.hits(),
            1,
            "a digest mismatch is deterministic; re-downloading cannot fix it"
        );
    }

    /// 503 is the server saying "busy", so the next attempt gets the archive.
    #[test]
    fn server_unavailable_is_retried() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let good = b"complete archive bytes".to_vec();
        let server = FakeServer::start(vec![
            Reply::Status(503, None),
            Reply::Body(good.clone()),
        ]);

        download_verified(&server.url(), &dest, Some(&md5_hex(&good))).expect("second attempt");
        assert_eq!(std::fs::read(&dest).expect("read back"), good);
        assert_eq!(server.hits(), 2);
    }

    /// A 404 is a verdict. It must surface as an HTTP status — not as a network
    /// error, which is what it looked like while every status was folded into
    /// `Network` — and it must be requested exactly once.
    #[test]
    fn not_found_is_reported_once_and_as_a_status() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let server = FakeServer::start(vec![Reply::Status(404, None)]);

        let err = download_file(&server.url(), &dest).unwrap_err();
        match err {
            ArunaError::Http { status, .. } => assert_eq!(status, 404),
            other => panic!("expected an HTTP status, got: {other}"),
        }
        assert_eq!(server.hits(), 1, "a 404 must not be re-requested");
    }

    /// A server that says when to come back is obeyed, within reason.
    #[test]
    fn retry_after_is_read_and_capped() {
        let err = |secs| ArunaError::Http {
            url: "u".into(),
            status: 429,
            retry_after: secs,
        };
        assert_eq!(retry_delay(1, &err(Some(5))), Duration::from_secs(5));
        assert_eq!(retry_delay(1, &err(Some(9_999))), MAX_RETRY_AFTER);
        // Without a header, backoff grows instead of staying flat.
        assert!(retry_delay(2, &err(None)) > retry_delay(1, &err(None)));
    }

    #[test]
    fn retryable_statuses_are_the_transient_ones() {
        for status in [408, 425, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(status), "{status} should be retried");
        }
        for status in [400, 401, 403, 404, 410, 451, 501] {
            assert!(!is_retryable_status(status), "{status} must not be retried");
        }
    }

    /// A matching digest passes the file through untouched.
    #[test]
    fn matching_checksum_is_accepted() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let body = b"the real archive".to_vec();
        let server = FakeServer::with_bodies(vec![Some(body.clone())]);

        download_verified(&server.url(), &dest, Some(&md5_hex(&body))).expect("accepted");
        assert_eq!(std::fs::read(&dest).expect("read back"), body);
        assert_eq!(server.hits(), 1, "no retry needed on success");
    }

    /// The retry policy, stated as a whole: transient failures get another go,
    /// settled ones are reported at once.
    #[test]
    fn retry_policy() {
        let http = |status| ArunaError::Http {
            url: "u".into(),
            status,
            retry_after: None,
        };
        assert!(!is_retryable(&http(404)));
        assert!(is_retryable(&http(503)));
        assert!(is_retryable(&ArunaError::Truncated {
            url: "u".into(),
            expected: 2,
            got: 1,
        }));
        assert!(!is_retryable(&ArunaError::ChecksumMismatch {
            url: "u".into(),
            expected: "a".into(),
            got: "b".into(),
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

}
