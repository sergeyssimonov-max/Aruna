//! Download the TLHdig ZIP from Zenodo.

use crate::error::{ArunaError, Result};
use crate::job::{Job, Phase};
use crate::md5::Md5;
use crate::progress::Event;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// Longest one attempt may take, counted from the request to the last byte.
///
/// The read timeout ([`DOWNLOAD_TIMEOUTS`]) bounds a single read, not the
/// transfer: a server that dribbles a byte before each read timeout keeps the
/// connection alive for as long as it likes, and the program sits there looking
/// frozen. This is the ceiling on that, and it is kept by [`stream_to_file`]
/// between reads of the body. It is not handed to the HTTP client as its
/// overall timeout: ureq 2 lets an overall timeout override the read timeout,
/// and until 2026-09-22 that is how the read timeout this comment promised was
/// in force nowhere. The response head is bounded by the connect timeout and by
/// the read timeout on each read, and ureq refuses a head over 100 KiB.
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

/// How often the transfer says how far it has got.
///
/// Only a sink drawing a window reads these; [`crate::progress::Stderr`] drops
/// them. The interval is the whole of the batching decision the contract asks
/// to be stated — `docs/FRONTEND-CONTRACT.md` §3 — and it is an interval rather
/// than a count of chunks because a chunk is 64 KiB of wire, not of progress:
/// the same file would tick 1 141 times on a fast link and four times on a slow
/// one, which describes the network instead of the download.
const TRANSFER_TICK: Duration = Duration::from_millis(250);

/// The most a metadata answer may be. Zenodo's record documents run to a few
/// tens of KiB; this is two orders of magnitude above that, and the point of it
/// is that a repository answering a question with a gigabyte is not answering
/// the question.
///
/// Public because the binary reads it: [`ArunaError::Oversized`] is raised
/// against two different ceilings, and the advice a reader is given has to say
/// which one was hit.
pub const MAX_METADATA: u64 = 4 * 1024 * 1024;

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
    download_verified_within(url, dest, expected_md5, job, DOWNLOAD_TIMEOUTS)
}

/// How long one attempt may wait, given as the attempt's own numbers.
///
/// A parameter rather than two constants read in place, so a test can hold a
/// server silent for a second instead of for the production read timeout.
#[derive(Debug, Clone, Copy)]
struct Timeouts {
    /// Longest wait for any single read: the response head, or the next piece
    /// of the body.
    read: Duration,
    /// Longest one attempt may take, head and body together.
    attempt: Duration,
}

/// The timeouts every download runs with.
///
/// Thirty seconds for one read, where it used to say five minutes and mean
/// fifteen: the read timeout is also how soon a cancel reaches a server that
/// has gone silent, because nothing else can interrupt a read that is waiting.
/// Set 2026-09-22 at the owner's word.
const DOWNLOAD_TIMEOUTS: Timeouts = Timeouts {
    read: Duration::from_secs(30),
    attempt: ATTEMPT_DEADLINE,
};

/// As [`download_verified`], with the timeouts given.
fn download_verified_within(
    url: &str,
    dest: &Path,
    expected_md5: Option<&str>,
    job: &Job<'_>,
    timeouts: Timeouts,
) -> Result<()> {
    let mut attempt = 1;
    loop {
        // Before an attempt, and again inside the body loop below. A run
        // cancelled between attempts must not start the next one — the whole
        // point of stopping a download is not to fetch the 71 MiB.
        job.check(Phase::Obtaining)?;
        match attempt_download(url, dest, expected_md5, job, timeouts) {
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
///
/// Read by `app::Failure` as well, which is why it is not private: a front end
/// that offers *Retry* has to agree with the client that does the retrying, and
/// it agreed by restating the set as a pattern of its own until the two lists
/// drifted apart over 501 and 505.
pub(crate) fn is_retryable_status(status: u16) -> bool {
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
pub(crate) fn is_retryable_io(err: &std::io::Error) -> bool {
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
///
/// The one answer to that question in this crate. `app::Failure` asks it rather
/// than judging for itself — see [`is_retryable_status`].
pub(crate) fn is_retryable(err: &ArunaError) -> bool {
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
    timeouts: Timeouts,
) -> Result<()> {
    create_parent(dest)?;
    let deadline = Instant::now() + timeouts.attempt;
    let response = request_within(url, timeouts)?;

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
    let transfer = stream_bounded(
        response.into_reader(),
        limit,
        announced,
        scratch.path(),
        (url, deadline),
        job,
    )?;
    transfer.verify(url, limit, announced, expected_md5)?;
    scratch.commit(dest)
}

/// Stream a body to `path`, refusing to write more than `limit` of it.
///
/// The bound lives here and nowhere else, so there is one line to get right and
/// one place to test. One byte past the limit is read on purpose: it is what
/// tells a body that runs over apart from one that ends exactly on it, and
/// [`Transfer::verify`] is what turns that byte into a refusal.
fn stream_bounded(
    reader: impl Read,
    limit: u64,
    announced: Option<u64>,
    path: &Path,
    from: (&str, Instant),
    job: &Job<'_>,
) -> Result<Transfer> {
    stream_to_file(
        &mut reader.take(limit.saturating_add(1)),
        announced,
        path,
        from,
        job,
    )
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
    std::fs::create_dir_all(parent).map_err(ArunaError::io(&parent))
}

/// Start one download attempt: the response head, within `timeouts.read`.
///
/// No overall `timeout` on this agent, and that is the point. ureq 2 lets an
/// overall timeout take precedence over the read timeout: it sets the socket's
/// read timeout to whatever is left of the overall one (`stream.rs` of the
/// crate), so with both set the read timeout was never in force — a server
/// that went silent held the attempt, and a cancel with it, until the attempt
/// deadline, fifteen minutes. The attempt deadline is kept by
/// [`stream_to_file`] between reads instead.
///
/// A proxy named in the environment is used – see [`proxy_from_env`].
/// `NO_PROXY` is not supported by `ureq` 2. An application started from
/// Finder sees no shell variables, so this serves the console.
fn request_within(url: &str, timeouts: Timeouts) -> Result<ureq::Response> {
    let agent = with_proxy(
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(timeouts.read)
            .user_agent(&user_agent()),
    )
    .build();
    call(&agent, url)
}

/// Fetch a small document as text, for asking questions rather than moving data.
///
/// Bounded twice over: by `deadline`, and by refusing a body larger than a
/// metadata response has any business being. A repository answering a question
/// with a gigabyte is not answering the question.
pub fn fetch_text(url: &str, deadline: Duration) -> Result<String> {
    fetch_text_within(url, deadline, MAX_METADATA)
}

/// The limit as an argument, so the boundary can be tested with six bytes
/// rather than four mebibytes.
///
/// **Refusing is the point, and it used to truncate instead.** `take(limit)`
/// stops reading and hands back what it has, so an answer one byte over the cap
/// arrived as its first `limit` bytes with nothing to say it had been cut. The
/// only caller parses the result as JSON ([`crate::zenodo`]), and JSON cut
/// mid-token fails to parse — so the truncation surfaced as "the repository
/// sent something that is not a record", which is a false account of what
/// happened. [`crate::export::validate::read_bounded`] had the same fault and
/// the same fix: read one byte past the cap, and having got it is proof the
/// body is over.
fn fetch_text_within(url: &str, deadline: Duration, limit: u64) -> Result<String> {
    let response = request_answer(url, deadline)?;

    // Bytes first, text after the length check, and in that order for a
    // reason: `read_to_string` validates UTF-8 across the whole read, so a body
    // cut one byte past the cap in the middle of a multi-byte character failed
    // as `InvalidData` — and came back as `Network`, which is retryable, rather
    // than as the refusal this function exists to give. A record document full
    // of Hittite sigla is exactly where that cut lands mid-character.
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| ArunaError::Network {
            url: url.to_string(),
            source: Box::new(source),
        })?;

    if bytes.len() as u64 > limit {
        return Err(ArunaError::Oversized {
            url: url.to_string(),
            limit,
            got: bytes.len() as u64,
        });
    }

    // An answer that is not text at all is still refused — a captive portal
    // answering with binary is the ordinary way that happens — but now it is
    // refused for being binary rather than for being long.
    String::from_utf8(bytes).map_err(|source| ArunaError::Network {
        url: url.to_string(),
        source: Box::new(source),
    })
}

/// What this program calls itself to a server.
///
/// One place, and derived: [`request_within`] and [`request_answer`] set it,
/// and the test below is what keeps it from drifting back into a literal.
pub fn user_agent() -> String {
    format!(
        "Aruna/{} (+https://github.com/sergeyssimonov-max/Aruna)",
        env!("CARGO_PKG_VERSION")
    )
}

/// The agent with the environment's proxy, if one is named and usable.
///
/// Both agents take it, as the download and the question about it must go the
/// same way behind a proxy. One that is named and not usable is passed over
/// here and reported by the caller that has a job to report to
/// ([`unusable_proxy`]).
fn with_proxy(builder: ureq::AgentBuilder) -> ureq::AgentBuilder {
    match proxy_among(|name| std::env::var(name).ok()).map(ureq::Proxy::new) {
        Some(Ok(proxy)) => builder.proxy(proxy),
        _ => builder,
    }
}

/// Which proxy the environment names for a request, as `curl` would choose it.
///
/// Chosen here rather than by `ureq`'s `try_proxy_from_env`, which asked
/// `ALL_PROXY` first and took the first value it could parse. Clash and Surge
/// export `https_proxy=http://…` beside `all_proxy=socks5://…`, and this crate
/// is built without SOCKS: every connection failed "SOCKS feature disabled",
/// where before T7 (2b24bf8) it had gone direct (review, 25.09.2026). Now the
/// order is `curl`'s for an `https` address – `HTTPS_PROXY`, then `ALL_PROXY` –
/// with `HTTP_PROXY` last, and only a proxy this crate can speak is taken: an
/// `http://` address or one written with no scheme. A SOCKS address is passed
/// over for the next variable.
fn proxy_among(var: impl Fn(&str) -> Option<String>) -> Option<String> {
    match proxy_choice(var) {
        ProxyChoice::Use(value) => Some(value),
        ProxyChoice::Unusable(_) | ProxyChoice::Direct => None,
    }
}

/// The variables a proxy is asked of, in the order they are asked.
pub const PROXY_VARIABLES: [&str; 6] = [
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "HTTP_PROXY",
    "http_proxy",
];

/// What the environment says about a proxy.
#[derive(Debug, PartialEq, Eq)]
enum ProxyChoice {
    /// This address, the first usable one in [`PROXY_VARIABLES`] order.
    Use(String),
    /// Something is named, nothing of it usable: requests go direct, and the
    /// run says which variable it passed over.
    Unusable(&'static str),
    /// Nothing is named.
    Direct,
}

fn proxy_choice(var: impl Fn(&str) -> Option<String>) -> ProxyChoice {
    let named: Vec<(&'static str, String)> = PROXY_VARIABLES
        .iter()
        .filter_map(|name| Some((*name, var(name)?.trim().to_string())))
        .filter(|(_, value)| !value.is_empty())
        .collect();
    match named.iter().find(|(_, value)| usable_proxy(value)) {
        Some((_, value)) => ProxyChoice::Use(value.clone()),
        None => match named.first() {
            Some((name, _)) => ProxyChoice::Unusable(name),
            None => ProxyChoice::Direct,
        },
    }
}

/// Whether this crate can reach a proxy at `value`: `http://` or no scheme,
/// an address `ureq` parses, and a port that is a number. `ureq` reads a port
/// that is not one as 80 without a word, and does not take an IPv6 literal.
fn usable_proxy(value: &str) -> bool {
    let rest = match value.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
        Some(_) => return false,
        None => value,
    };
    let host = rest
        .rsplit('@')
        .next()
        .unwrap_or(rest)
        .trim_end_matches('/');
    let port_ok = match host.rsplit_once(':') {
        Some((_, port)) => !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()),
        None => true,
    };
    !host.starts_with('[') && port_ok && ureq::Proxy::new(value).is_ok()
}

/// The proxy variable the environment names and this crate cannot use, when
/// it names no usable one – so the run can say it goes direct (owner's
/// decision, 25.09.2026: direct, and said, rather than refused).
pub fn unusable_proxy() -> Option<&'static str> {
    match proxy_choice(|name| std::env::var(name).ok()) {
        ProxyChoice::Unusable(name) => Some(name),
        ProxyChoice::Use(_) | ProxyChoice::Direct => None,
    }
}

/// GET `url` under one overall deadline – for asking questions ([`fetch_text`]),
/// where the answer is small and nobody is waiting to cancel it.
fn request_answer(url: &str, deadline: Duration) -> Result<ureq::Response> {
    // Every wait is held to the deadline as well, not only the whole: ureq 2
    // does not keep the overall timeout over a TLS handshake through a proxy,
    // and there the read timeout was what applied – 300 s against a 10 s
    // question (release gate 25.09.2026, Н1).
    let agent = with_proxy(ureq::AgentBuilder::new())
        .timeout_connect(deadline.min(Duration::from_secs(30)))
        .timeout_read(deadline)
        .timeout(deadline)
        // Built from the manifest rather than written out. It said `Aruna/1.0`
        // through every release of the 2.x line — a version string that stopped
        // being true at v1.0.9 and would have gone on being wrong forever,
        // because nothing ever fails when it is. Zenodo's logs are the one place
        // this shows, which is exactly why nobody would have noticed.
        .user_agent(&user_agent())
        .build();
    call(&agent, url)
}

/// GET `url`, turning a refusal into the error that describes it.
///
/// ureq hands back every non-2xx as `Error::Status`, so the status has to be
/// pulled out of the error rather than off a response. Mapping the whole error
/// to `Network` — as this did — buried the status: a 404 was reported as a
/// network failure and, because network failures are retried, fetched three
/// times before the user heard about it.
fn call(agent: &ureq::Agent, url: &str) -> Result<ureq::Response> {
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
///
/// `from` is where the body comes from and when this attempt has to be over.
/// A failure to read is the network's and is reported against that URL; only a
/// failure to write is this machine's and is reported against `path`. Until
/// 2026-09-22 both went out as [`ArunaError::Io`] with the scratch path, and a
/// body the server cut short reached the window as "the disk could not take the
/// file – usually no space or no permission".
fn stream_to_file(
    reader: &mut impl Read,
    announced: Option<u64>,
    path: &Path,
    (url, deadline): (&str, Instant),
    job: &Job<'_>,
) -> Result<Transfer> {
    let io = |source| ArunaError::Io {
        path: path.to_path_buf(),
        source,
    };
    let network = |source: std::io::Error| ArunaError::Network {
        url: url.to_string(),
        source: Box::new(source),
    };

    let mut file = File::create(path).map_err(io)?;
    let mut bytes: u64 = 0;
    let mut digest = Md5::new();
    let mut buf = [0u8; 64 * 1024];
    // Told by time rather than by chunk, and this is the seam the contract asks
    // to be named (`docs/FRONTEND-CONTRACT.md` §3: batch by count or by
    // interval, and say which). By chunk would be 1 141 events for 71 MiB on a
    // fast link and four on a slow one — the rate would say how quick the
    // network is, not how far the transfer got. A quarter-second is fast enough
    // that a bar never looks stuck and slow enough that the events cost nothing.
    let mut ticked = Instant::now();
    // Сколько байт названо последним тиком: остаток досказывается только если
    // он есть. Ноль здесь – не «ничего не сказано», а «сказано про ноль», и для
    // пустого тела досказывать нечего.
    let mut reported: u64 = 0;

    let outcome = loop {
        // Between chunks of 64 KiB. The scratch file is dropped on the way out
        // — `Scratch` removes an uncommitted one — so a cancelled download
        // leaves nothing behind, which is exactly what a failed one does.
        if job.is_cancelled() {
            return Err(ArunaError::Cancelled {
                phase: Phase::Obtaining,
            });
        }
        // The stall guard for a body that keeps arriving too slowly to finish:
        // each read is bounded by the read timeout, the attempt by this.
        if Instant::now() >= deadline {
            return Err(network(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "the attempt ran past its deadline",
            )));
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => n,
            // Прерванное чтение – не оборванная передача. `read` возвращает
            // `Interrupted` вместо того, чтобы повторить самому, и ответом на
            // один прерванный кусок в 64 КиБ была повторная выкачка 71 МиБ:
            // повтор стоял уровнем выше, чем нужно. Отмена при этом не
            // теряется – ее спрашивают в начале каждого круга.
            Err(source) if source.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(source) => return Err(network(source)),
        };
        if let Err(source) = file.write_all(&buf[..n]) {
            break Err(source);
        }
        digest.update(&buf[..n]);
        bytes += n as u64;
        if ticked.elapsed() >= TRANSFER_TICK {
            ticked = Instant::now();
            reported = bytes;
            job.report(Event::Downloading {
                bytes,
                total: announced,
            });
        }
    };
    // Остаток, чтобы последнее, что слышно о передаче, было ею целиком.
    //
    // Тик говорится по времени, поэтому тело, пришедшее внутри одного
    // промежутка, не давало ни одного события: знаменатель стадия объявила, а
    // числителя не приходило никогда, и на повторе он вдобавок откатывался к
    // нулю внутри той же стадии. Пишущий проход досказывает остаток той же
    // формой и по той же причине (`export::write_documents`), включая условие:
    // передача, чей последний тик уже назвал целое, не говорит этого дважды.
    if outcome.is_ok() && reported != bytes {
        job.report(Event::Downloading {
            bytes,
            total: announced,
        });
    }
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
        std::fs::rename(&self.path, dest).map_err(ArunaError::io(dest))?;
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

    /// A deadline no test here reaches, for the tests that are about something
    /// other than time.
    fn far_deadline() -> Instant {
        Instant::now() + Duration::from_secs(3600)
    }

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
        /// Hold the connection open and send nothing, not even a status line:
        /// the server that has stopped answering.
        Silent,
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
                        Some(Reply::Silent) => {
                            std::thread::sleep(std::time::Duration::from_secs(60));
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
    /// reading and reports a closed body as a read error – a network failure
    /// since 2026-09-22, an I/O error on the scratch file before that – and our
    /// own count catches the case where the reader ends cleanly instead. Both
    /// are retryable and both must leave `dest` alone — that is what matters
    /// here.
    #[test]
    fn truncated_body_is_rejected() {
        let dir = tempdir().expect("tempdir");
        let dest = dir.path().join("out.zip");
        let server = FakeServer::with_bodies(vec![None]);

        let err = download_file(&server.url(), &dest, &Job::unattended()).unwrap_err();
        assert!(
            matches!(
                err,
                ArunaError::Truncated { .. } | ArunaError::Network { .. }
            ),
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

    /// **A question behind a proxy that went silent is given up on by its own
    /// deadline.**
    ///
    /// The proxy answers CONNECT and then says nothing, so the TLS handshake
    /// waits on a read. ureq 2 does not hold the overall timeout over that
    /// handshake: the read timeout is what applies there, and it was 300 s
    /// against the 10 s the Zenodo question asks for – measured 300 s by the
    /// release gate of 25.09.2026 (Н1), with a cancel unable to reach it. The
    /// proxy here holds the line for 20 s, so the old code fails in 20 s
    /// rather than 300.
    #[test]
    fn a_question_behind_a_silent_proxy_ends_by_its_deadline() {
        use std::io::{Read as _, Write as _};
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let _ = conn.read(&mut buf).expect("read");
            conn.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .expect("reply");
            std::thread::sleep(Duration::from_secs(20));
        });

        std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{port}"));
        let started = std::time::Instant::now();
        let answer = fetch_text(
            "https://aruna-proxy-test.invalid/record",
            Duration::from_secs(1),
        );
        let waited = started.elapsed();
        std::env::remove_var("HTTPS_PROXY");

        assert!(answer.is_err(), "a silent proxy gave an answer");
        assert!(
            waited < Duration::from_secs(5),
            "a 1 s question waited {waited:?}"
        );
    }

    /// **A proxy named in the environment carries the request.**
    ///
    /// Until 2026-09-24 nothing read the proxy variables: behind a proxy the download
    /// could not go anywhere, and there was nothing to set. The host here does
    /// not resolve, so without the proxy the request fails at DNS; with it the
    /// request line reaches the proxy in absolute form. nextest runs each test
    /// in its own process, so the variable set here reaches no other test.
    #[test]
    fn a_proxy_named_in_the_environment_carries_the_request() {
        use std::io::{Read as _, Write as _};
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let proxy = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let n = conn.read(&mut buf).expect("read");
            let line = String::from_utf8_lossy(&buf[..n])
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("reply");
            line
        });

        // `HTTPS_PROXY`, the first one asked: a proxy the developer's own
        // shell names in any other variable cannot take the request instead.
        std::env::set_var("HTTPS_PROXY", format!("http://127.0.0.1:{port}"));
        let answer = fetch_text(
            "http://aruna-proxy-test.invalid/record",
            Duration::from_secs(10),
        );
        std::env::remove_var("HTTPS_PROXY");

        assert_eq!(answer.expect("the answer through the proxy"), "ok");
        assert!(
            proxy
                .join()
                .expect("proxy")
                .starts_with("GET http://aruna-proxy-test.invalid/record"),
            "the request did not go through the proxy"
        );
    }

    /// **The proxy is chosen as `curl` chooses it, and SOCKS is passed over.**
    #[test]
    fn the_proxy_is_chosen_as_curl_chooses_it() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        // Clash and Surge: both set, and the SOCKS one must not win.
        assert_eq!(
            proxy_among(env(&[
                ("all_proxy", "socks5://127.0.0.1:7890"),
                ("https_proxy", "http://127.0.0.1:7890"),
            ])),
            Some("http://127.0.0.1:7890".into())
        );
        // `HTTPS_PROXY` before `ALL_PROXY`, and both before `HTTP_PROXY`.
        assert_eq!(
            proxy_among(env(&[
                ("HTTP_PROXY", "http://c:3"),
                ("ALL_PROXY", "http://b:2"),
                ("HTTPS_PROXY", "a:1"),
            ])),
            Some("a:1".into())
        );
        // SOCKS alone: no proxy, as before T7, rather than no connection.
        assert_eq!(proxy_among(env(&[("ALL_PROXY", "socks5://x:1")])), None);
        // A scheme this crate cannot speak is passed over for the next.
        assert_eq!(
            proxy_among(env(&[
                ("https_proxy", "https://p:3128"),
                ("http_proxy", "http://q:8080")
            ])),
            Some("http://q:8080".into())
        );
        assert_eq!(proxy_among(env(&[("HTTPS_PROXY", "  ")])), None);
        assert_eq!(proxy_among(env(&[])), None);
    }

    /// **A proxy that is named and cannot be used is said, not hidden**, and
    /// the variable is named; one that can be used says nothing.
    #[test]
    fn a_proxy_that_cannot_be_used_is_named() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        for (pairs, expected) in [
            (
                &[("ALL_PROXY", "socks5://x:1")][..],
                ProxyChoice::Unusable("ALL_PROXY"),
            ),
            (
                &[("https_proxy", "https://p:3128")][..],
                ProxyChoice::Unusable("https_proxy"),
            ),
            (
                &[("HTTPS_PROXY", "http://p:eighty")][..],
                ProxyChoice::Unusable("HTTPS_PROXY"),
            ),
            (
                &[("HTTPS_PROXY", "http://[::1]:3128")][..],
                ProxyChoice::Unusable("HTTPS_PROXY"),
            ),
            (
                &[("HTTPS_PROXY", "http://user:pw@p:3128/")][..],
                ProxyChoice::Use("http://user:pw@p:3128/".into()),
            ),
            (
                &[("ALL_PROXY", "socks5://x:1"), ("http_proxy", "q:8080")][..],
                ProxyChoice::Use("q:8080".into()),
            ),
            (&[("HTTPS_PROXY", " ")][..], ProxyChoice::Direct),
            (&[][..], ProxyChoice::Direct),
        ] {
            let pairs: &'static [(&'static str, &'static str)] = pairs;
            assert_eq!(proxy_choice(env(pairs)), expected, "{pairs:?}");
        }
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
        let transfer = stream_bounded(
            std::io::repeat(b'x'),
            limit,
            None,
            &scratch,
            ("test://", far_deadline()),
            &Job::unattended(),
        )
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

    /// The last thing a window hears about the transfer is the whole of it.
    ///
    /// The tick is told by time, so a body that arrives inside one interval
    /// produced no `Downloading` event at all: the stage had announced a
    /// denominator and no numerator ever followed. The write pass says the same
    /// thing the other way round and has said it since it was written —
    /// `export::write_documents` emits the remainder so that the last fraction a
    /// window hears is `documents / documents`, and
    /// `tests/progress_flow.rs::the_write_pass_ticks_are_a_fraction_that_only_grows`
    /// pins it. The transfer had no such counterpart.
    #[test]
    fn the_last_word_on_a_transfer_is_the_whole_of_it() {
        #[derive(Default)]
        struct Heard(std::sync::Mutex<Vec<(u64, Option<u64>)>>);
        impl crate::progress::Progress for Heard {
            fn report(&self, event: Event<'_>) {
                if let Event::Downloading { bytes, total } = event {
                    self.0.lock().expect("lock").push((bytes, total));
                }
            }
        }

        let dir = tempdir().expect("tempdir");
        let scratch = dir.path().join("short.part");
        let body = b"a body that arrives inside one tick".to_vec();
        let heard = Heard::default();
        let cancel = crate::job::Cancel::new();
        let job = Job::new(&heard, &cancel);

        let transfer = stream_bounded(
            std::io::Cursor::new(body.clone()),
            MAX_DOWNLOAD,
            Some(body.len() as u64),
            &scratch,
            ("test://", far_deadline()),
            &job,
        )
        .expect("the transfer succeeds");

        assert_eq!(transfer.bytes, body.len() as u64);
        let ticks = heard.0.lock().expect("lock").clone();
        assert_eq!(
            ticks.last().copied(),
            Some((body.len() as u64, Some(body.len() as u64))),
            "the transfer never said it had arrived: {ticks:?}"
        );
        // И ровно один раз: тело пришло внутри одного промежутка, так что
        // остаток — единственное, что о нём сказано.
        assert_eq!(ticks.len(), 1, "the whole was reported twice: {ticks:?}");
    }

    /// One interrupted read is not a failed download.
    ///
    /// `read` returns `ErrorKind::Interrupted` instead of retrying, and this
    /// loop treated it as the end of the transfer. The retry then sat one level
    /// too high: the answer to one interrupted 64 KiB read was fetching 71 MiB
    /// again, and three of them inside one run failed it outright.
    /// `is_retryable_io` above already names the kind — it was only never
    /// answered here.
    #[test]
    fn an_interrupted_read_does_not_lose_the_transfer() {
        /// Yields `Interrupted` once, then the body, then the end.
        struct Twitchy {
            interrupted: bool,
            rest: std::io::Cursor<Vec<u8>>,
        }
        impl Read for Twitchy {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
                }
                self.rest.read(buf)
            }
        }

        let dir = tempdir().expect("tempdir");
        let scratch = dir.path().join("twitchy.part");
        let body = b"the whole body, delivered after one interruption".to_vec();

        let transfer = stream_bounded(
            Twitchy {
                interrupted: false,
                rest: std::io::Cursor::new(body.clone()),
            },
            MAX_DOWNLOAD,
            Some(body.len() as u64),
            &scratch,
            ("test://", far_deadline()),
            &Job::unattended(),
        )
        .expect("an interrupted read is not a failed transfer");

        assert_eq!(transfer.bytes, body.len() as u64);
        assert_eq!(
            std::fs::read(&scratch).expect("scratch"),
            body,
            "the body on disk is the body that was served"
        );
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
    fn a_flood_is_refused_by_size() {
        let flood = vec![b'x'; 8 * 1024 * 1024];
        let server = FakeServer::start(vec![Reply::Body(flood)]);
        let err = fetch_text(&server.url(), Duration::from_secs(30))
            .expect_err("a flood is not an answer");
        assert!(
            matches!(err, ArunaError::Oversized { limit, .. } if limit == MAX_METADATA),
            "an oversized answer came back as {err}"
        );
    }

    /// A body cut mid-character is refused as too long, not as not-text.
    ///
    /// Three two-byte characters against a three-byte cap: the read stops
    /// inside the second one. Reading straight into a `String` failed there on
    /// the invalid UTF-8 and reported a network error — retryable, and about
    /// the wrong thing.
    #[test]
    fn a_cut_inside_a_character_is_still_a_refusal() {
        let server = FakeServer::start(vec![Reply::Body("ααα".as_bytes().to_vec())]);
        let err = fetch_text_within(&server.url(), Duration::from_secs(5), 3)
            .expect_err("six bytes against a three byte cap");
        assert!(
            matches!(
                err,
                ArunaError::Oversized {
                    limit: 3,
                    got: 4,
                    ..
                }
            ),
            "a cut inside a character came back as {err}"
        );
    }

    /// The cap refuses rather than truncates, and it refuses one byte over it.
    ///
    /// Exactly at the cap is an answer; a byte past it is not, and the caller
    /// hears that instead of being handed a prefix it cannot tell from the
    /// whole.
    #[test]
    fn the_metadata_cap_is_a_refusal_and_not_a_prefix() {
        let server = FakeServer::start(vec![Reply::Body(b"<html>".to_vec())]);
        assert_eq!(
            fetch_text_within(&server.url(), Duration::from_secs(5), 6).expect("exactly the cap"),
            "<html>"
        );

        let server = FakeServer::start(vec![Reply::Body(b"<html>".to_vec())]);
        let err = fetch_text_within(&server.url(), Duration::from_secs(5), 5)
            .expect_err("one byte over the cap");
        assert!(
            matches!(
                err,
                ArunaError::Oversized {
                    limit: 5,
                    got: 6,
                    ..
                }
            ),
            "one byte over came back as {err}"
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
    /// The deadline is asked for as a whole and asserted on as a whole: one
    /// attempt, request and body, the way the program runs it. Since 2026-09-22
    /// the attempt deadline is kept between reads of the body rather than by
    /// ureq's overall timeout, which overrode the read timeout.
    ///
    /// Which step the deadline bites at is a fact about the machine. That it
    /// bites, and that the program is back in well under five seconds, is the
    /// fact about the program — so that is what is asserted.
    #[test]
    fn a_server_that_dribbles_for_ever_is_given_up_on() {
        let server = FakeServer::start(vec![Reply::Dribble]);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("archive.zip");
        let timeouts = Timeouts {
            read: Duration::from_secs(1),
            attempt: Duration::from_millis(400),
        };

        let started = std::time::Instant::now();
        let outcome = attempt_download(&server.url(), &dest, None, &Job::unattended(), timeouts);
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

    /// Отмена доходит до загрузки, чей сервер замолчал, не позже таймаута чтения.
    ///
    /// До 22.09.2026 не доходила до конца попытки. Агенту задавался и таймаут
    /// чтения, и общий срок попытки, а в ureq 2 общий срок берет верх: таймаут
    /// сокета ставится равным времени до конца попытки (`stream.rs` крейта),
    /// и заявленные 300 секунд на одно чтение не действовали нигде. Отмену же
    /// спрашивают только между кусками тела и между попытками. Сервер, который
    /// принял соединение и молчит, держал кнопку «Остановить» до пятнадцати
    /// минут – замерено макетом: отмена на пятой секунде не вернулась и за 120.
    #[test]
    fn a_cancel_reaches_a_download_whose_server_went_silent() {
        let server = FakeServer::start(vec![Reply::Silent]);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("archive.zip");
        let cancel = crate::job::Cancel::new();
        let job = Job::new(&crate::progress::Silent, &cancel);
        let later = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            later.cancel();
        });
        let timeouts = Timeouts {
            read: Duration::from_secs(1),
            attempt: Duration::from_secs(30),
        };

        let started = Instant::now();
        let err = download_verified_within(&server.url(), &dest, None, &job, timeouts)
            .expect_err("a silent server gives nothing to download");
        let waited = started.elapsed();

        assert!(
            matches!(err, ArunaError::Cancelled { .. }),
            "unexpected: {err}"
        );
        assert!(
            waited < Duration::from_secs(5),
            "the cancel was heard after {waited:?}, not within the read timeout"
        );
        assert!(!dest.exists());
    }

    /// Фаза заголовков ограничена и без общего срока попытки.
    ///
    /// Общий `timeout` агента снят 22.09.2026, и вместе с ним ушло то, что
    /// ограничивало ожидание ответа целиком. Теперь его ограничивает таймаут
    /// чтения: сервер, принявший соединение и не сказавший ни байта, отпускает
    /// попытку через него, и отказ – сетевой, то есть повторяемый. Отрицательный
    /// контроль сделан при добавлении: с возвращенным `.timeout(attempt)` тест
    /// ждет весь срок попытки и падает.
    #[test]
    fn a_server_that_never_answers_is_given_up_on_within_the_read_timeout() {
        let server = FakeServer::start(vec![Reply::Silent]);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("archive.zip");
        let timeouts = Timeouts {
            read: Duration::from_secs(1),
            attempt: Duration::from_secs(30),
        };

        let started = Instant::now();
        let err = attempt_download(&server.url(), &dest, None, &Job::unattended(), timeouts)
            .expect_err("a server that says nothing gives nothing to download");
        let waited = started.elapsed();

        assert!(
            matches!(err, ArunaError::Network { .. }),
            "unexpected: {err}"
        );
        assert!(
            is_retryable(&err),
            "a silent server deserves another attempt"
        );
        assert!(
            waited < Duration::from_secs(5),
            "the head was waited for {waited:?}, past the read timeout"
        );
        assert!(!dest.exists());
    }

    /// Сервер, оборвавший тело, – отказ сети, а не диска.
    ///
    /// Ошибка чтения из ответа и ошибка записи в файл шли одной дорогой, в
    /// `ArunaError::Io` с путем черновика, и окно говорило читателю: «Диску не
    /// удалось отдать или принять файл. Чаще всего это нехватка места или нет
    /// прав на папку» – о передаче, которую оборвал сервер. Повтор при этом был
    /// верным и остается: меняется только то, что сказано.
    #[test]
    fn a_body_cut_by_the_server_is_a_network_failure_not_a_disk_one() {
        let server = FakeServer::with_bodies(vec![None]);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.zip");

        let err = download_file(&server.url(), &dest, &Job::unattended()).unwrap_err();

        assert!(
            !matches!(err, ArunaError::Io { .. }),
            "a cut body was reported as a local file error: {err}"
        );
        assert_ne!(crate::app::Failure::of(&err).code, "io");
        assert!(is_retryable(&err), "a short body deserves another attempt");
        assert!(!dest.exists());
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
