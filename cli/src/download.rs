//! Download the TLHdig ZIP from Zenodo.

use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use crate::md5::Md5;
use crate::progress::Event;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Stable Zenodo URL for TLHdig Beta 0.3.
pub const ZENODO_ZIP_URL: &str =
    "https://zenodo.org/records/20328284/files/TLHbasisONLINE25_1_ZENODO_Beta_03.zip?download=1";

/// MD5 published by Zenodo for record 20328284, verified against the fixture.
///
/// If Zenodo ever republishes the archive this has to be updated together with
/// [`ZENODO_ZIP_URL`]; the mismatch error prints both digests, so the diagnosis
/// is immediate.
///
/// **What a republish costs, decided before it happens.** Every copy of this
/// program already installed stops working on that day: a moved record answers
/// 404 and a re-cut file fails this digest, and neither is recoverable on the
/// reader's machine — the pin is compiled in. So the answer is a release, not a
/// note in an issue:
///
/// 1. take the new URL and the MD5 Zenodo publishes beside it, and update both
///    constants here in one commit — they are a pair, and a URL updated alone
///    turns a clear 404 into a mismatch that looks like corruption;
/// 2. replace `cli/fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip` with the new
///    archive, so the corpus job and the fixture tests measure what readers get,
///    and re-record any digest the tests carry;
/// 3. expect the corpus numbers to move. 663 groups and 23 936 documents are
///    asserted in several places, and a republished corpus is a different
///    corpus; the numbers are updated deliberately, with the new ones read off
///    a real run, and never loosened into ranges to avoid the work;
/// 4. cut a release. `main.rs` sends both failures at the reader to
///    `releases/latest`, which is the only fix they can apply.
///
/// A republish that changes only packaging (same documents, new digest) still
/// takes all four steps; only step 3 comes back unchanged.
pub const ZENODO_ZIP_MD5: &str = "f9acbc8db3111cc7dd88d82f7819a912";

/// Attempts per download before giving up.
const MAX_ATTEMPTS: u32 = 3;

/// Download `url` into `dest`, retrying transient failures.
///
/// No integrity check — see [`download_verified`] for that.
pub fn download_file(url: &str, dest: &Path, job: &Job<'_>) -> Result<()> {
    download_verified(url, dest, None, job)
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

/// The most this program will ever write for one download.
///
/// Only reached when the server announces no size at all; a server that states
/// one is held to what it stated. Fourteen times the archive as it stands
/// (71 MiB), so a corpus that keeps growing does not walk into it, and far
/// short of any volume it would be reasonable to fill.
const MAX_DOWNLOAD: u64 = 1024 * 1024 * 1024;

/// Download `url` into `dest`, retrying transient failures and rejecting the
/// result unless it hashes to `expected_md5`.
///
/// Retries cover the failures a second attempt can actually fix: a dropped
/// connection, a short body, a local write error, and the HTTP statuses that
/// mean "busy, not wrong". See [`is_retryable`] for what is deliberately left
/// out.
pub fn download_verified(
    url: &str,
    dest: &Path,
    expected_md5: Option<&str>,
    job: &Job<'_>,
) -> Result<()> {
    let mut attempt = 1;
    loop {
        // Before an attempt, and again inside the body loop below. A run
        // cancelled between attempts must not start the next one — the whole
        // point of stopping a download is not to fetch the 71 MiB.
        job.check(Phase::Obtaining)?;
        match attempt_download(url, dest, expected_md5, job) {
            Ok(()) => return Ok(()),
            Err(err) if attempt < MAX_ATTEMPTS && is_retryable(&err) => {
                let delay = retry_delay(attempt, &err);
                job.report(Event::DownloadRetrying {
                    attempt,
                    delay,
                    error: &err,
                });
                // Slept in slices so a cancelled run does not sit out a
                // backoff nobody is waiting for any more. Sixteen seconds is
                // the longest this waits, and a person who clicked Cancel
                // should not watch it out.
                if sleep_unless_cancelled(delay, job).is_err() {
                    return Err(ArunaError::Cancelled {
                        phase: Phase::Obtaining,
                    });
                }
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
        // A redirect loop is settled, not transient. Measured before this
        // arm existed: a server redirecting to itself was walked five times
        // by the client, then the whole thing was retried twice more — 15
        // requests and eight seconds, six of them spent asleep, to reach the
        // identical error. The same reasoning as the digest mismatch below.
        ArunaError::Network { source, .. } if is_redirect_loop(source.as_ref()) => false,
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

/// Whether a transport failure is the client giving up on a redirect chain.
///
/// Asked of the boxed source by type rather than by message: the wording
/// belongs to the HTTP client and would change without warning, and matching a
/// string is how a check quietly stops matching anything.
fn is_redirect_loop(source: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    matches!(
        source.downcast_ref::<ureq::Error>(),
        Some(ureq::Error::Transport(transport))
            if transport.kind() == ureq::ErrorKind::TooManyRedirects
    )
}

/// How long to wait before the next attempt.
///
/// A server that sent `Retry-After` has told us what it wants, and it is
/// honoured as sent — spreading out a wait the server itself chose would be
/// second-guessing the one party that knows.
///
/// Everything else gets exponential backoff plus a spread of up to a quarter of
/// it. Aruna is run by hand rather than in a fleet, so the spread is not about
/// this process: it is about all the copies of it that were reading from Zenodo
/// when Zenodo started answering 503, and that would otherwise come back at the
/// same two-second and four-second marks together. A quarter is enough to break
/// the lockstep while leaving the wait roughly as long as it says it is.
fn retry_delay(attempt: u32, err: &ArunaError) -> Duration {
    if let ArunaError::Http {
        retry_after: Some(secs),
        ..
    } = err
    {
        return Duration::from_secs(*secs).min(MAX_RETRY_AFTER);
    }
    let base = Duration::from_secs(2u64.saturating_pow(attempt));
    base + base.mul_f64(0.25 * spread())
}

/// A number in `[0, 1)` to spread a backoff with.
///
/// The clock rather than a random number generator: this decides how long to
/// pause before retrying a download, and a dependency — or a hand-rolled
/// generator with state to keep — would be a great deal of machinery for that.
/// Nanoseconds since the last second are as unrelated between two machines as
/// this needs them to be, and nothing here is security-sensitive.
fn spread() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos) / 1_000_000_000.0
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
fn attempt_download(
    url: &str,
    dest: &Path,
    expected_md5: Option<&str>,
    job: &Job<'_>,
) -> Result<()> {
    create_parent(dest)?;
    let response = request(url)?;

    // Announced size, when the server sends one — used below to catch a body
    // cut short by a dropped connection.
    let announced: Option<u64> = response
        .header("Content-Length")
        .and_then(|v| v.trim().parse().ok());

    // A body this program will not accept is refused before a byte of it is
    // written, rather than after the disk has taken all of it.
    let limit = download_limit(url, announced)?;

    // Stream into a scratch file and rename only on success: an interrupted
    // download must never leave a truncated archive sitting at `dest` looking
    // like a complete one.
    let scratch = Scratch::beside(dest);
    let transfer = stream_bounded(response.into_reader(), limit, scratch.path(), job)?;
    transfer.verify(url, limit, announced, expected_md5)?;
    scratch.commit(dest)
}

/// Stream a body to `path`, refusing to write more than `limit` of it.
///
/// The bound lives here and nowhere else, so there is one line to get right and
/// one place to test. One byte past the limit is read on purpose: it is what
/// tells a body that runs over apart from one that ends exactly on it, and
/// [`Transfer::verify`] is what turns that byte into a refusal.
fn stream_bounded(reader: impl Read, limit: u64, path: &Path, job: &Job<'_>) -> Result<Transfer> {
    stream_to_file(&mut reader.take(limit.saturating_add(1)), path, job)
}

/// Wait out `delay`, unless the run is cancelled while waiting.
///
/// Polled rather than parked on a condition variable: the flag is an atomic and
/// nothing signals it, so there is nothing to wait on. A tenth of a second is
/// far below what a person notices and far above what costs anything — sixteen
/// seconds of backoff is 160 loads.
fn sleep_unless_cancelled(delay: Duration, job: &Job<'_>) -> std::result::Result<(), ()> {
    const SLICE: Duration = Duration::from_millis(100);
    let deadline = std::time::Instant::now() + delay;
    while std::time::Instant::now() < deadline {
        if job.is_cancelled() {
            return Err(());
        }
        std::thread::sleep(
            SLICE.min(deadline.saturating_duration_since(std::time::Instant::now())),
        );
    }
    if job.is_cancelled() {
        return Err(());
    }
    Ok(())
}

/// The most this transfer may write, and a refusal if that is already too much.
///
/// Two things are bounded here, and they are not the same thing. A server that
/// announces a size is held to it — anything past it is a body that disagrees
/// with its own header, and there is no reason to keep writing it to disk. A
/// server that announces nothing gets [`MAX_DOWNLOAD`], because a transfer with
/// no stated end and no ceiling is a transfer that stops when the disk is full.
///
/// Without this the only limit was the digest check, which happens after the
/// last byte has been written: an endless body filled the volume first and was
/// rejected afterwards.
fn download_limit(url: &str, announced: Option<u64>) -> Result<u64> {
    match announced {
        Some(size) if size > MAX_DOWNLOAD => Err(ArunaError::Oversized {
            url: url.to_string(),
            limit: MAX_DOWNLOAD,
            got: size,
        }),
        Some(size) => Ok(size),
        None => Ok(MAX_DOWNLOAD),
    }
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

/// Fetch a small document as text, for asking questions rather than moving data.
///
/// Bounded twice over: by `deadline`, and by refusing a body larger than a
/// metadata response has any business being. A repository answering a question
/// with a gigabyte is not answering the question.
pub fn fetch_text(url: &str, deadline: Duration) -> Result<String> {
    /// Zenodo's record documents run to a few tens of KiB.
    const MAX_METADATA: u64 = 4 * 1024 * 1024;

    let response = request_within(url, deadline)?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_METADATA)
        .read_to_string(&mut body)
        .map_err(|source| ArunaError::Network {
            url: url.to_string(),
            source: Box::new(source),
        })?;
    Ok(body)
}

/// What this program calls itself to a server.
///
/// One place, and derived: [`request_within`] sets it, and the test below is
/// what keeps it from drifting back into a literal.
pub fn user_agent() -> String {
    format!(
        "Aruna/{} (+https://github.com/sergeyssimonov-max/Aruna)",
        env!("CARGO_PKG_VERSION")
    )
}

/// As [`request`], with the deadline given — the tests need one they can wait for.
fn request_within(url: &str, deadline: Duration) -> Result<ureq::Response> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(300))
        .timeout(deadline)
        // Built from the manifest rather than written out. It said `Aruna/1.0`
        // through every release of the 2.x line — a version string that stopped
        // being true at v1.0.9 and would have gone on being wrong forever,
        // because nothing ever fails when it is. Zenodo's logs are the one place
        // this shows, which is exactly why nobody would have noticed.
        .user_agent(&user_agent())
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
    fn verify(
        self,
        url: &str,
        limit: u64,
        announced: Option<u64>,
        expected_md5: Option<&str>,
    ) -> Result<()> {
        // Asked first: past the limit the transfer was cut short deliberately,
        // so every other check below would be reading a truncated body and
        // reporting the wrong thing about it.
        if self.bytes > limit {
            return Err(ArunaError::Oversized {
                url: url.to_string(),
                limit,
                got: self.bytes,
            });
        }

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
fn stream_to_file(reader: &mut impl Read, path: &Path, job: &Job<'_>) -> Result<Transfer> {
    let io = |source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    };

    let mut file = File::create(path).map_err(io)?;
    let mut bytes: u64 = 0;
    let mut digest = Md5::new();
    let mut buf = [0u8; 64 * 1024];

    let outcome = loop {
        // Between chunks of 64 KiB. The scratch file is dropped on the way out
        // — `Scratch` removes an uncommitted one — so a cancelled download
        // leaves nothing behind, which is exactly what a failed one does.
        if job.is_cancelled() {
            return Err(ArunaError::Cancelled {
                phase: Phase::Obtaining,
            });
        }
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
        /// Send a body with no `Content-Length`, ended by closing the
        /// connection. The transport cannot bound this one, so it is the shape
        /// [`MAX_DOWNLOAD`] is the only limit on.
        Unannounced(Vec<u8>),
    }

    /// A one-shot HTTP server that serves `replies[i]` to request `i`, counting
    /// the requests it saw. Local so the retry tests neither touch the network
    /// nor wait on real timeouts.
    struct FakeServer {
        port: u16,
        hits: Arc<AtomicU32>,
        /// The request heads it was sent, so a test can ask what the client
        /// said about itself as well as what it asked for.
        heads: Arc<std::sync::Mutex<Vec<String>>>,
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
            let heads: Arc<std::sync::Mutex<Vec<String>>> = Default::default();
            let recorder = Arc::clone(&heads);

            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let i = counter.fetch_add(1, Ordering::SeqCst) as usize;
                    let reply = replies.get(i).or_else(|| replies.last());

                    let mut stream = stream;
                    // Read the request line so the client is not writing into a
                    // closed socket while we answer.
                    let mut head = String::new();
                    let mut line = String::new();
                    let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                    while reader.read_line(&mut line).unwrap_or(0) > 2 {
                        head.push_str(&line);
                        line.clear();
                    }
                    if let Ok(mut seen) = recorder.lock() {
                        seen.push(head);
                    }

                    match reply {
                        None | Some(Reply::Truncated) => {
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\n\r\nshort");
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
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\n\r\n");
                            let _ = stream.flush();
                            // Slowly enough to outlast any sane deadline, and
                            // for ever: the client must be the one to give up.
                            while stream.write_all(b"x").is_ok() {
                                let _ = stream.flush();
                                std::thread::sleep(std::time::Duration::from_millis(20));
                            }
                        }
                        Some(Reply::Unannounced(bytes)) => {
                            let _ =
                                stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                            let _ = stream.write_all(bytes);
                            let _ = stream.flush();
                            let _ = stream.shutdown(std::net::Shutdown::Write);
                        }
                        Some(Reply::Status(status, retry_after)) => {
                            let mut head =
                                format!("HTTP/1.1 {status} Something\r\nContent-Length: 0\r\n");
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

            FakeServer { port, hits, heads }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}/archive.zip", self.port)
        }

        fn hits(&self) -> u32 {
            self.hits.load(Ordering::SeqCst)
        }

        /// The head of the first request, headers and all.
        fn first_head(&self) -> String {
            self.heads
                .lock()
                .expect("the recorder is not poisoned")
                .first()
                .cloned()
                .unwrap_or_default()
        }
    }

    /// The program tells a server which version of itself is asking.
    ///
    /// It said `Aruna/1.0` from 1.0.0 through 2.3.0 — true for one release and
    /// wrong for every one after, and invisible, because a wrong User-Agent
    /// never fails anything. Zenodo's logs are where it shows, which is the one
    /// place nobody here can read. It is built from the manifest now, and this
    /// test is what keeps it from being written out by hand again: it asks a
    /// real server what arrived.
    #[test]
    fn the_user_agent_carries_the_version_from_the_manifest() {
        let server = FakeServer::with_bodies(vec![Some(b"body".to_vec())]);
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        download_file(&server.url(), &dest, &Job::unattended()).expect("download");

        let head = server.first_head();
        let expected = format!("User-Agent: {}", user_agent());
        assert!(
            head.contains(&expected),
            "the request should carry {expected:?}; it carried:\n{head}"
        );
        assert!(
            user_agent().contains(env!("CARGO_PKG_VERSION")),
            "and the version in it is the manifest's, not a literal"
        );
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

        let err = download_file(&server.url(), &dest, &Job::unattended()).unwrap_err();
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

        download_verified(
            &server.url(),
            &dest,
            Some(&md5_hex(&good)),
            &Job::unattended(),
        )
        .expect("second attempt");
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

        let err = download_verified(
            &server.url(),
            &dest,
            Some(&md5_hex(b"expected")),
            &Job::unattended(),
        )
        .unwrap_err();
        match err {
            ArunaError::ChecksumMismatch { expected, got, .. } => {
                assert_eq!(expected, md5_hex(b"expected"));
                assert_eq!(got, md5_hex(b"corrupted"));
            }
            other => panic!("unexpected: {other}"),
        }
        assert!(
            !dest.exists(),
            "corrupt body must not reach the destination"
        );
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
        let server = FakeServer::start(vec![Reply::Status(503, None), Reply::Body(good.clone())]);

        download_verified(
            &server.url(),
            &dest,
            Some(&md5_hex(&good)),
            &Job::unattended(),
        )
        .expect("second attempt");
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

        let err = download_file(&server.url(), &dest, &Job::unattended()).unwrap_err();
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

        download_verified(
            &server.url(),
            &dest,
            Some(&md5_hex(&body)),
            &Job::unattended(),
        )
        .expect("accepted");
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

    /// The metadata request is an aside: every way it can fail must come back
    /// as an error the caller can shrug off, never as a panic or a hang.
    #[test]
    fn asking_a_question_fails_as_an_error_rather_than_a_surprise() {
        // A refusal is a refusal, whatever the status.
        for status in [404u16, 410, 500, 503] {
            let server = FakeServer::start(vec![Reply::Status(status, None)]);
            let err = fetch_text(&server.url(), Duration::from_secs(5))
                .expect_err("a status is not an answer");
            assert!(
                matches!(err, ArunaError::Http { status: got, .. } if got == status),
                "status {status} came back as {err}"
            );
        }

        // An answer that is not JSON is still handed over: refusing it is the
        // caller's job, and it is the caller that knows what it asked for.
        let server = FakeServer::start(vec![Reply::Body(b"<html>502</html>".to_vec())]);
        assert_eq!(
            fetch_text(&server.url(), Duration::from_secs(5)).unwrap(),
            "<html>502</html>"
        );
    }

    /// An answer that is not text at all is refused, not mangled.
    ///
    /// A captive portal or a proxy answering with binary is the ordinary way
    /// this happens on a hotel network.
    #[test]
    fn an_answer_that_is_not_text_is_refused() {
        let server = FakeServer::start(vec![Reply::Body(vec![0xff, 0xfe, 0x00, 0x01])]);
        assert!(
            fetch_text(&server.url(), Duration::from_secs(5)).is_err(),
            "bytes that are not UTF-8 are not an answer"
        );
    }

    /// A fast flood is capped by size, where the endless dribble below is
    /// capped by time. Both matter: only one of them is slow.
    /// A body with no end writes a bounded amount and is then refused.
    ///
    /// This is the case the limit exists for. A response that states a
    /// `Content-Length` is already held to it by the transport, so the gap was
    /// the other kind — chunked, or closed by the server — where the read runs
    /// until EOF and there is no EOF. Nothing stood between that and the volume
    /// being written to except the digest check, which happens after the last
    /// byte, and there is no last byte.
    ///
    /// Driven through the streaming path with a small limit rather than through
    /// a server with the real one: the point is that the write stops, and
    /// proving it with a gigabyte would be the same proof at ten thousand times
    /// the cost.
    #[test]
    fn a_body_with_no_end_is_written_only_up_to_the_limit() {
        let dir = tempdir().expect("tempdir");
        let scratch = dir.path().join("endless.part");
        let limit = 64 * 1024;

        // `repeat` never returns 0, exactly like a connection that keeps
        // delivering. Nothing bounds it here but the function under test.
        let transfer = stream_bounded(std::io::repeat(b'x'), limit, &scratch, &Job::unattended())
            .expect("the write itself succeeds");

        assert_eq!(
            transfer.bytes,
            limit + 1,
            "one byte past the limit, which is how the overrun is detected"
        );
        assert_eq!(
            std::fs::metadata(&scratch).expect("scratch exists").len(),
            limit + 1,
            "the disk took a bounded amount and no more"
        );

        match transfer.verify("u", limit, None, None) {
            Err(ArunaError::Oversized { limit: l, got, .. }) => {
                assert_eq!(l, limit);
                assert_eq!(got, limit + 1);
            }
            other => panic!("expected Oversized, got {other:?}"),
        }
    }

    /// A response with no stated length still downloads, and still verifies.
    ///
    /// This is the shape the ceiling applies to, so it is also the shape the
    /// ceiling could have broken: reading under a limit must not turn a body
    /// that ends by closing the connection into a truncated one.
    #[test]
    fn a_response_with_no_stated_length_still_arrives_whole() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("archive.zip");
        let payload = b"not a zip, but all of it".to_vec();
        let server = FakeServer::start(vec![Reply::Unannounced(payload.clone())]);

        download_verified(
            &server.url(),
            &dest,
            Some(&crate::md5::md5_hex(&payload)),
            &Job::unattended(),
        )
        .expect("an unannounced body is a complete body");

        assert_eq!(std::fs::read(&dest).expect("dest"), payload);
        assert!(leftover_parts(dir.path()).is_empty());
    }

    /// The failed attempt takes its scratch file with it, whatever went wrong.
    #[test]
    fn a_refused_download_leaves_nothing_behind() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("archive.zip");
        let server = FakeServer::start(vec![Reply::Truncated]);

        assert!(download_file(&server.url(), &dest, &Job::unattended()).is_err());
        assert!(!dest.exists(), "nothing was committed to the destination");
        assert!(
            leftover_parts(dir.path()).is_empty(),
            "a scratch file outlived the attempt"
        );
    }

    /// A size the program will not accept is refused from the header alone,
    /// without opening a file to write it into.
    #[test]
    fn an_announced_size_over_the_ceiling_is_refused_before_the_body() {
        let url = "https://example.invalid/huge.zip";
        match download_limit(url, Some(MAX_DOWNLOAD + 1)) {
            Err(ArunaError::Oversized { limit, got, .. }) => {
                assert_eq!(limit, MAX_DOWNLOAD);
                assert_eq!(got, MAX_DOWNLOAD + 1);
            }
            other => panic!("expected Oversized, got {other:?}"),
        }

        // A server that says nothing gets the ceiling rather than no limit.
        assert_eq!(download_limit(url, None).ok(), Some(MAX_DOWNLOAD));
        // One that states a workable size is held to exactly that.
        assert_eq!(download_limit(url, Some(71 << 20)).ok(), Some(71 << 20));
    }

    /// Scratch files matching `paths::scratch_sibling`, which are what an
    /// abandoned attempt leaves behind.
    fn leftover_parts(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "part"))
            .collect()
    }

    /// The backoff spreads, and stays inside the window it advertises: long
    /// enough to be the wait it says it is, never so long that three attempts
    /// become a hang.
    #[test]
    fn the_backoff_is_spread_without_drifting_out_of_its_window() {
        let err = ArunaError::EmptyArchive;
        for attempt in 1..MAX_ATTEMPTS {
            let base = Duration::from_secs(2u64.saturating_pow(attempt));
            for _ in 0..64 {
                let delay = retry_delay(attempt, &err);
                assert!(
                    delay >= base,
                    "attempt {attempt}: {delay:?} is below {base:?}"
                );
                assert!(
                    delay <= base.mul_f64(1.25),
                    "attempt {attempt}: {delay:?} is more than a quarter over {base:?}"
                );
            }
        }
    }

    /// `Retry-After` is honoured as sent rather than spread: the server named
    /// the moment it wants to be asked again, and it is still capped.
    #[test]
    fn a_retry_after_is_taken_at_its_word_and_capped() {
        let told = |secs| {
            retry_delay(
                1,
                &ArunaError::Http {
                    url: "u".into(),
                    status: 503,
                    retry_after: Some(secs),
                },
            )
        };
        assert_eq!(told(7), Duration::from_secs(7));
        assert_eq!(told(9_999), MAX_RETRY_AFTER);
    }

    #[test]
    fn a_flood_is_capped_by_size() {
        let flood = vec![b'x'; 8 * 1024 * 1024];
        let server = FakeServer::start(vec![Reply::Body(flood)]);
        let body = fetch_text(&server.url(), Duration::from_secs(30)).expect("read succeeds");
        assert!(
            body.len() <= 4 * 1024 * 1024,
            "read {} bytes, which is not a cap",
            body.len()
        );
    }

    /// A repository answering a small question with an endless body must not
    /// be able to exhaust memory: the read is capped.
    #[test]
    fn an_oversized_answer_is_cut_rather_than_swallowed() {
        let server = FakeServer::start(vec![Reply::Dribble]);
        let started = std::time::Instant::now();
        // The dribble never ends; the deadline is what stops it, and the cap is
        // what would stop a fast flood.
        let outcome = fetch_text(&server.url(), Duration::from_millis(400));
        assert!(outcome.is_err(), "an endless answer must not be waited out");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "gave up after {:?}",
            started.elapsed()
        );
    }

    /// A server that never stops sending must not stop the program either.
    ///
    /// `timeout_read` cannot catch this: every read returns a byte, so no
    /// single read ever times out. Without an overall deadline the download sat
    /// there for as long as the server cared to dribble — for ever, in this
    /// test — and the only way out was killing the process.
    ///
    /// The deadline is asked for as a whole and asserted on as a whole. It used
    /// to be split: the request was `expect`ed to succeed and only the body was
    /// allowed to fail, which reads as two steps but is one budget —
    /// `request_within` sets ureq's overall timeout, and that covers the
    /// headers and the body together. So a machine that did not schedule the
    /// server's accept loop inside 400 ms failed the test on `expect`, and
    /// under this suite's own parallelism that happened about one run in seven.
    ///
    /// Which step the deadline bites at is a fact about the machine. That it
    /// bites, and that the program is back in well under five seconds, is the
    /// fact about the program — so that is what is asserted.
    #[test]
    fn a_server_that_dribbles_for_ever_is_given_up_on() {
        let server = FakeServer::start(vec![Reply::Dribble]);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("archive.zip");

        let started = std::time::Instant::now();
        let outcome =
            request_within(&server.url(), Duration::from_millis(400)).and_then(|response| {
                stream_to_file(&mut response.into_reader(), &dest, &Job::unattended())
            });
        let waited = started.elapsed();

        assert!(
            outcome.is_err(),
            "a transfer that never ends must not be waited out"
        );
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
        let err =
            download_file("http://127.0.0.1:1/nope.zip", &dest, &Job::unattended()).unwrap_err();
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
        assert!(download_file("http://127.0.0.1:1/nope.zip", &dest, &Job::unattended()).is_err());
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
        assert!(download_file("http://127.0.0.1:1/nope.zip", &dest, &Job::unattended()).is_err());
        assert_eq!(
            std::fs::read(&dest).expect("read back"),
            b"previous good archive"
        );
    }
}
