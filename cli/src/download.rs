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

/// Longest one attempt may take, headers and body together.
///
/// `timeout_read` below bounds a single read, not the transfer: a server that
/// dribbles a byte before each deadline keeps the connection alive for as long
/// as it likes, and the program sits there looking frozen with no way out but
/// Ctrl-C. This is the ceiling on that.
///
/// Fifteen minutes is 71 MiB at 79 KiB/s sustained — a floor no working
/// connection is under, and twelve times slower than the archive actually
/// arrives. A run that hits it says so and can be retried; before, it did not
/// end.
const ATTEMPT_DEADLINE: Duration = Duration::from_secs(15 * 60);

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

/// I/O failures worth another attempt.
///
/// An allowlist rather than a denylist, because the cost of guessing wrong is
/// asymmetric: retrying re-downloads 71 MiB. `UnexpectedEof` is the one that
/// matters — it is how ureq reports a body that stopped short of its
/// `Content-Length` — and the rest are ordinary interruptions.
///
/// Everything else is treated as settled, which is what a full disk, a
/// read-only volume, an exceeded quota or a permission error are. Those used to
/// be retried, so a run that could not write its scratch file downloaded the
/// archive three times before saying so.
///
/// Listing what may be retried rather than what may not also means a kind
/// nobody anticipated costs one download instead of three.
fn is_retryable_io(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::UnexpectedEof
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe
    )
}

/// Whether another attempt has any chance of succeeding.
fn is_retryable(err: &ArunaError) -> bool {
    match err {
        ArunaError::Network { .. } | ArunaError::Truncated { .. } => true,
        ArunaError::Io { source, .. } => is_retryable_io(source),
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

/// One transfer, in the four steps it actually has.
///
/// Written as four calls rather than one block because each step fails in its
/// own way and the reader has to be able to find the one they are looking at:
/// the request turns a status into [`ArunaError::Http`], the transfer streams
/// and hashes at once, the checks are what stands between a damaged body and
/// `dest`, and the rename is the only moment anything at `dest` changes.
///
/// The scratch file is not deleted anywhere in here. [`Scratch`] removes it
/// when it goes out of scope uncommitted, which covers every `?` above — the
/// previous version repeated the removal at four returns and had to be read in
/// full to be sure it covered all of them.
fn attempt_download(url: &str, dest: &Path, expected_md5: Option<&str>) -> Result<()> {
    create_parent(dest)?;
    let response = request(url)?;

    // Announced size, when the server sends one — used below to catch a body
    // cut short by a dropped connection.
    let announced: Option<u64> = response
        .header("Content-Length")
        .and_then(|v| v.trim().parse().ok());

    // Stream into a scratch file and rename only on success: an interrupted
    // download must never leave a truncated archive sitting at `dest` looking
    // like a complete one.
    let scratch = Scratch::beside(dest);
    let transfer = stream_to_file(&mut response.into_reader(), scratch.path())?;
    transfer.verify(url, announced, expected_md5)?;
    scratch.commit(dest)
}

/// Create the directory `dest` will be written into.
fn create_parent(dest: &Path) -> Result<()> {
    let Some(parent) = dest.parent() else {
        return Ok(());
    };
    std::fs::create_dir_all(parent).map_err(|source| ArunaError::Io {
        path: parent.to_path_buf(),
        source,
    })
}

/// GET `url`, turning a refusal into the error that describes it.
///
/// ureq hands back every non-2xx as `Error::Status`, so the status has to be
/// pulled out of the error rather than off a response. Mapping the whole error
/// to `Network` — as this did — buried the status: a 404 was reported as a
/// network failure and, because network failures are retried, fetched three
/// times before the user heard about it.
fn request(url: &str) -> Result<ureq::Response> {
    request_within(url, ATTEMPT_DEADLINE)
}

/// As [`request`], with the deadline given — the tests need one they can wait for.
fn request_within(url: &str, deadline: Duration) -> Result<ureq::Response> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(300))
        .timeout(deadline)
        .user_agent("Aruna/1.0 (+https://github.com/sergeyssimonov-max/Aruna)")
        .build();

    match agent.get(url).call() {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(status, response)) => Err(ArunaError::Http {
            url: url.to_string(),
            status,
            // Only the delta-seconds form is read. `Retry-After` may also carry
            // an HTTP-date, but parsing dates to shave a few seconds off a
            // backoff is not worth a date parser.
            retry_after: response
                .header("Retry-After")
                .and_then(|v| v.trim().parse().ok()),
        }),
        Err(source) => Err(ArunaError::Network {
            url: url.to_string(),
            source: Box::new(source),
        }),
    }
}

/// What arrived: how many bytes, and what they hash to.
struct Transfer {
    bytes: u64,
    digest: Md5,
}

impl Transfer {
    /// Reject a body that is short or does not hash as promised.
    ///
    /// Both checks run before the rename in [`attempt_download`], so a damaged
    /// body never reaches `dest` under a name that says it is the archive.
    fn verify(self, url: &str, announced: Option<u64>, expected_md5: Option<&str>) -> Result<()> {
        if let Some(announced) = announced {
            if self.bytes != announced {
                return Err(ArunaError::Truncated {
                    url: url.to_string(),
                    expected: announced,
                    got: self.bytes,
                });
            }
        }

        if let Some(expected) = expected_md5 {
            let got = self.digest.finish_hex();
            if !got.eq_ignore_ascii_case(expected) {
                return Err(ArunaError::ChecksumMismatch {
                    url: url.to_string(),
                    expected: expected.to_string(),
                    got,
                });
            }
        }
        Ok(())
    }
}

/// Copy `reader` into `path`, hashing as it goes.
///
/// Hashed in the same pass: a second read over 71 MiB just to digest the file
/// would cost more than the check it feeds.
fn stream_to_file(reader: &mut impl Read, path: &Path) -> Result<Transfer> {
    let io = |source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    };

    let mut file = File::create(path).map_err(io)?;
    let mut bytes: u64 = 0;
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
        bytes += n as u64;
    };
    // The data is only on disk once `sync_all` returns, and the file has to be
    // closed before the rename that follows.
    let outcome = outcome.and_then(|()| file.sync_all());
    drop(file);
    outcome.map_err(io)?;

    Ok(Transfer { bytes, digest })
}

/// The in-flight download: a scratch file that deletes itself unless committed.
///
/// The path is [`crate::paths::scratch_sibling`], the same convention the
/// inventory is written with: beside the destination, so the closing rename is
/// atomic, and carrying the process id, so concurrent runs do not share it.
struct Scratch {
    path: PathBuf,
    committed: bool,
}

impl Scratch {
    fn beside(dest: &Path) -> Self {
        Self {
            path: crate::paths::scratch_sibling(dest),
            committed: false,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    /// Move the finished file to `dest`.
    ///
    /// On failure the scratch file is dropped like any other uncommitted one —
    /// which is what a rename that could not happen means.
    fn commit(mut self, dest: &Path) -> Result<()> {
        std::fs::rename(&self.path, dest).map_err(|source| ArunaError::Io {
            path: dest.to_path_buf(),
            source,
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
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
        /// Announce a body and then send it a byte at a time, for ever: the
        /// stall that no per-read timeout can catch.
        Dribble,
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
                        Some(Reply::Dribble) => {
                            let _ = stream.write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\n\r\n",
                            );
                            let _ = stream.flush();
                            // Slowly enough to outlast any sane deadline, and
                            // for ever: the client must be the one to give up.
                            while stream.write_all(b"x").is_ok() {
                                let _ = stream.flush();
                                std::thread::sleep(std::time::Duration::from_millis(20));
                            }
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

    /// A local write that cannot succeed must not drag the archive down the
    /// wire again. Raw codes rather than `ErrorKind` names, so the test states
    /// the exact condition it means.
    #[test]
    fn a_failed_local_write_is_not_retried() {
        let io = |code| ArunaError::Io {
            path: "/tmp/x".into(),
            source: std::io::Error::from_raw_os_error(code),
        };
        for (code, what) in [
            (28, "ENOSPC — disk full"),
            (13, "EACCES — permission denied"),
            (30, "EROFS — read-only filesystem"),
            (69, "EDQUOT — quota exceeded"),
        ] {
            assert!(
                !is_retryable(&io(code)),
                "{what} cannot be fixed by downloading 71 MiB again"
            );
        }
    }

    /// The interruptions that a second attempt does fix stay retryable — above
    /// all the short body, which is how ureq reports a connection that dropped
    /// mid-transfer.
    #[test]
    fn an_interrupted_transfer_is_retried() {
        for kind in [
            std::io::ErrorKind::UnexpectedEof,
            std::io::ErrorKind::Interrupted,
            std::io::ErrorKind::TimedOut,
            std::io::ErrorKind::ConnectionReset,
        ] {
            assert!(
                is_retryable(&ArunaError::Io {
                    path: "/tmp/x".into(),
                    source: std::io::Error::new(kind, "interrupted"),
                }),
                "{kind:?} deserves another attempt"
            );
        }
    }

    /// A server that never stops sending must not stop the program either.
    ///
    /// `timeout_read` cannot catch this: every read returns a byte, so no
    /// single read ever times out. Without an overall deadline the download sat
    /// there for as long as the server cared to dribble — for ever, in this
    /// test — and the only way out was killing the process.
    #[test]
    fn a_server_that_dribbles_for_ever_is_given_up_on() {
        let server = FakeServer::start(vec![Reply::Dribble]);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("archive.zip");

        let started = std::time::Instant::now();
        // The two steps `attempt_download` takes, with a deadline a test can
        // wait for: the headers arrive at once, and the body never ends.
        let response = request_within(&server.url(), Duration::from_millis(400))
            .expect("headers are sent immediately");
        let outcome = stream_to_file(&mut response.into_reader(), &dest);
        let waited = started.elapsed();

        assert!(outcome.is_err(), "a transfer that never ends must not be waited out");
        assert!(
            waited < Duration::from_secs(5),
            "gave up after {waited:?}, which is not giving up"
        );
    }

    /// The deadline the program actually runs with is generous enough that no
    /// working connection meets it: 71 MiB at the floor it implies.
    #[test]
    fn the_deadline_is_a_stall_guard_not_a_speed_limit() {
        let archive_bytes = 74_449_198u64;
        let floor = archive_bytes / ATTEMPT_DEADLINE.as_secs();
        assert!(
            (60_000..200_000).contains(&floor),
            "the deadline implies {floor} B/s sustained, which is no longer a stall guard"
        );
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

    /// Both READMEs point the reader at the Zenodo record, by hand.
    ///
    /// They are the fifth and sixth copies of a number that lives in
    /// `ZENODO_ZIP_URL`, and the only ones a person reads before running
    /// anything. `SOURCE_LABEL` is already checked against the URL; this covers
    /// the documentation, so republishing the archive cannot leave the prose
    /// sending people to the record the tool no longer downloads.
    ///
    /// Included at compile time, so a moved or renamed README is a build error
    /// rather than a test that quietly stops checking anything.
    #[test]
    fn the_readmes_point_at_the_record_that_is_downloaded() {
        let record = ZENODO_ZIP_URL
            .split("/records/")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .expect("the Zenodo URL names a record");
        let landing = format!("https://zenodo.org/records/{record}");
        // The prose drops `?download=1`; the path before it is what must agree.
        let file_url = ZENODO_ZIP_URL.split('?').next().expect("split yields one");

        for (name, text) in [
            ("cli/README.md", include_str!("../README.md")),
            ("README.md", include_str!("../../README.md")),
        ] {
            assert!(
                text.contains(&landing),
                "{name} does not link {landing} — the record the CLI downloads"
            );
        }
        assert!(
            include_str!("../README.md").contains(file_url),
            "cli/README.md names a different archive than {file_url}"
        );
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
