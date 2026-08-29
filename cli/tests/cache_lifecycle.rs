//! The decision between a cached archive and a downloaded one.
//!
//! This is the difference between a two-second run and a one-minute one, and
//! until now nothing exercised it. Every other test reaches the parse through
//! `ARUNA_ZIP`, which hands the archive over directly and skips the whole
//! question — so `download`, `cache` and `md5` each had their own tests while
//! the code that wires them together had none. Coverage said so plainly:
//! `lib.rs` sat at 64 %, the lowest in the crate, and this is what was missing.
//!
//! Hermetic: a local server on a port the operating system picks, a temporary
//! directory for the cache, and an archive invented here. Nothing reaches
//! Zenodo, and the server counts its requests so "it used the cache" can be
//! asserted rather than assumed.
//!
//! That first claim was false until 2026-08-23 and nothing here showed it: the
//! download path asks Zenodo which edition of the corpus is current, so every
//! test below made a live request the local server never saw. `obtain_archive`
//! here is `support::obtain_archive`, which hands that lookup an answer instead
//! of a network.

mod support;

use aruna::cache::{self, Archive};
use aruna::error::ArunaError;
use aruna::md5::md5_hex;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use support::obtain_archive;
use tempfile::{tempdir, TempDir};

/// A server that serves one body and counts who asked.
///
/// Small on purpose: the download module has a thorough fake of its own for
/// testing HTTP behaviour, and what is needed here is only a place for the
/// archive to come from, plus the count that tells a cache hit from a second
/// download.
struct Origin {
    port: u16,
    hits: Arc<AtomicUsize>,
    /// Set when the test is done with the server; the accept loop reads it and
    /// stops. Without it the thread blocks on `accept` for ever and outlives
    /// the test, which nextest reports as a leak — correctly: a test that
    /// leaves a listening socket behind is a test that leaves a resource
    /// behind.
    stop: Arc<AtomicBool>,
}

impl Origin {
    /// A server that hands out one body, with a matching `Content-Length`.
    fn start(body: Vec<u8>) -> Self {
        Self::start_with(move |stream, _| Self::answer(stream, &body))
    }

    /// A server that sends everyone somewhere else.
    ///
    /// `None` means "back to myself", which is the loop a client has to be able
    /// to give up on.
    fn redirecting(to: Option<String>) -> Self {
        Self::start_with(move |stream, port| {
            let location = to
                .clone()
                .unwrap_or_else(|| format!("http://127.0.0.1:{port}/loop"));
            let head = format!(
                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.flush();
        })
    }

    fn start_with(reply: impl Fn(&mut TcpStream, u16) + Send + 'static) -> Self {
        // Port 0: the operating system picks a free one, so concurrent tests
        // cannot collide over a hard-coded number.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    return;
                }
                let Ok(mut stream) = stream else { break };
                counter.fetch_add(1, Ordering::SeqCst);
                // Read the request line so the client is not writing into a
                // socket nobody is reading.
                let mut scratch = [0u8; 1024];
                let _ = stream.read(&mut scratch);
                reply(&mut stream, port);
            }
        });

        Origin { port, hits, stop }
    }

    fn answer(stream: &mut TcpStream, body: &[u8]) {
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/TLHbasis.zip", self.port)
    }

    fn requests(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

impl Drop for Origin {
    fn drop(&mut self) {
        // Raise the flag, then knock on the door: `accept` is blocking, so it
        // has to be woken before it can notice.
        self.stop.store(true, Ordering::SeqCst);
        let _ = std::net::TcpStream::connect(("127.0.0.1", self.port));
    }
}

/// A cache directory of this test's own, and the environment pointing at it.
///
/// `ARUNA_CACHE_DIR` is process-wide, so the tests that set it are kept in one
/// module and run one at a time — see `CACHE_ENV` below.
struct CacheDir {
    dir: TempDir,
}

impl CacheDir {
    fn new() -> Self {
        CacheDir {
            dir: tempdir().expect("tempdir"),
        }
    }

    fn path(&self) -> std::path::PathBuf {
        self.dir.path().join("cache")
    }

    fn files(&self) -> Vec<String> {
        std::fs::read_dir(self.path())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect()
    }
}

/// An archive body: not a real ZIP, because nothing here parses it. What
/// matters is that it has a digest and a length.
fn body() -> Vec<u8> {
    b"PK\x03\x04 pretend this is seventy-one megabytes of Hittite".to_vec()
}

/// `ARUNA_CACHE_DIR` is read from the process environment, and these tests each
/// need a different value. Setting it from several threads at once is a race
/// no assertion could survive, so they take turns.
static CACHE_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Run `f` with the cache directory pointed at `dir`, and put the environment
/// back afterwards whatever happens.
fn with_cache_dir<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    let _guard = CACHE_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var_os(cache::CACHE_DIR_ENV);
    std::env::set_var(cache::CACHE_DIR_ENV, dir);

    let outcome = f();

    match previous {
        Some(value) => std::env::set_var(cache::CACHE_DIR_ENV, value),
        None => std::env::remove_var(cache::CACHE_DIR_ENV),
    }
    outcome
}

#[test]
fn a_cold_run_downloads_and_a_warm_one_does_not() {
    let payload = body();
    let digest = md5_hex(&payload);
    let origin = Origin::start(payload.clone());
    let cache = CacheDir::new();

    let (first, second) = with_cache_dir(&cache.path(), || {
        let first = obtain_archive(&origin.url(), &digest, &aruna::job::Job::unattended())
            .expect("cold run downloads");
        let second = obtain_archive(&origin.url(), &digest, &aruna::job::Job::unattended())
            .expect("warm run is served");
        (first, second)
    });

    // Both runs got an archive, and the cold one kept it.
    assert!(
        matches!(first, Archive::Cached(_)),
        "the download was not kept"
    );
    assert!(matches!(second, Archive::Cached(_)));
    assert_eq!(
        std::fs::read(first.path()).expect("read"),
        payload,
        "the cached archive is not what the server sent"
    );

    assert_eq!(
        origin.requests(),
        1,
        "the second run went back to the network instead of using the cache"
    );
    assert_eq!(
        cache.files().len(),
        1,
        "the cache holds {:?} rather than one archive",
        cache.files()
    );
}

/// The digest is the authority, and a body that does not match it is not
/// allowed to become the cached archive.
#[test]
fn an_archive_that_hashes_wrong_is_refused_and_not_cached() {
    let origin = Origin::start(body());
    let cache = CacheDir::new();

    let outcome = with_cache_dir(&cache.path(), || {
        obtain_archive(
            &origin.url(),
            "00000000000000000000000000000000",
            &aruna::job::Job::unattended(),
        )
    });

    match outcome {
        Err(ArunaError::ChecksumMismatch { expected, got, .. }) => {
            assert_eq!(expected, "00000000000000000000000000000000");
            assert_eq!(
                got,
                md5_hex(&body()),
                "the message names what actually arrived"
            );
        }
        Err(other) => panic!("expected a checksum mismatch, got {other}"),
        Ok(_) => panic!("an archive with the wrong digest was accepted"),
    }

    assert!(
        cache.files().is_empty(),
        "a rejected archive was left in the cache: {:?}",
        cache.files()
    );
}

/// A digest that no longer matches what the server publishes leaves the old
/// archive behind; the sweep is what stops the cache growing by 71 MiB every
/// time the corpus is republished.
#[test]
fn a_republished_archive_replaces_the_edition_it_supersedes() {
    let first_payload = body();
    let origin = Origin::start(first_payload.clone());
    let cache = CacheDir::new();

    let second_payload = b"PK\x03\x04 a later edition of the same archive".to_vec();
    let later = Origin::start(second_payload.clone());

    let files = with_cache_dir(&cache.path(), || {
        obtain_archive(
            &origin.url(),
            &md5_hex(&first_payload),
            &aruna::job::Job::unattended(),
        )
        .expect("first edition");
        let before = std::fs::read_dir(cache.path()).expect("read").count();

        // The same file name at the source, a different digest: what a
        // republished record looks like.
        let url = format!("http://127.0.0.1:{}/TLHbasis.zip", later.port);
        obtain_archive(
            &url,
            &md5_hex(&second_payload),
            &aruna::job::Job::unattended(),
        )
        .expect("second edition");
        (before, cache.files())
    });

    assert_eq!(files.0, 1, "the first edition was not cached");
    assert_eq!(
        files.1.len(),
        1,
        "the superseded edition is still there: {:?}",
        files.1
    );
}

/// A cache that cannot be written is a reason to do without one, never a reason
/// to fail a run. The archive still arrives — it is simply not kept.
#[cfg(unix)]
#[test]
fn an_unwritable_cache_directory_falls_back_to_a_run_of_its_own() {
    use std::os::unix::fs::PermissionsExt;

    let payload = body();
    let origin = Origin::start(payload.clone());
    let cache = CacheDir::new();
    std::fs::create_dir_all(cache.path()).expect("mkdir");
    std::fs::set_permissions(cache.path(), std::fs::Permissions::from_mode(0o500)).expect("chmod");

    let outcome = with_cache_dir(&cache.path(), || {
        obtain_archive(
            &origin.url(),
            &md5_hex(&payload),
            &aruna::job::Job::unattended(),
        )
    });

    std::fs::set_permissions(cache.path(), std::fs::Permissions::from_mode(0o755)).expect("chmod");

    let archive = outcome.expect("an unusable cache must not fail the run");
    assert!(
        matches!(archive, Archive::Temporary(_)),
        "the archive was reported as cached in a cache that cannot be written"
    );
    assert_eq!(
        std::fs::read(archive.path()).expect("read"),
        payload,
        "the fallback archive is not what the server sent"
    );
    assert!(cache.files().is_empty());
}

/// A server that is not there is an error, not a hang, and nothing is left in
/// the cache to be mistaken for an archive later.
#[test]
fn an_unreachable_source_fails_without_leaving_anything_behind() {
    let cache = CacheDir::new();
    // Bind and drop: the port is real and certainly closed.
    let dead = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = dead.local_addr().expect("addr").port();
    drop(dead);

    let started = std::time::Instant::now();
    let outcome = with_cache_dir(&cache.path(), || {
        obtain_archive(
            &format!("http://127.0.0.1:{port}/x.zip"),
            &md5_hex(&body()),
            &aruna::job::Job::unattended(),
        )
    });

    assert!(outcome.is_err(), "an unreachable source reported success");
    assert!(
        started.elapsed() < Duration::from_secs(90),
        "gave up only after {:?}",
        started.elapsed()
    );
    assert!(
        cache.files().is_empty(),
        "a failed download left {:?} in the cache",
        cache.files()
    );
}

/// A redirect is followed, and what arrives is still held to its digest.
///
/// Zenodo answers the archive URL with a redirect in practice, and the client
/// follows it — but nothing here had ever checked that, nor that the digest is
/// still the authority afterwards. A redirect that quietly delivered something
/// else would otherwise be indistinguishable from a successful download.
#[test]
fn a_redirect_is_followed_and_the_result_still_has_to_hash_right() {
    let payload = body();
    let real = Origin::start(payload.clone());
    let front = Origin::redirecting(Some(real.url()));
    let cache = CacheDir::new();

    let archive = with_cache_dir(&cache.path(), || {
        obtain_archive(
            &front.url(),
            &md5_hex(&payload),
            &aruna::job::Job::unattended(),
        )
    })
    .expect("a redirect leads to the archive");

    assert_eq!(std::fs::read(archive.path()).expect("read"), payload);
    assert_eq!(front.requests(), 1, "the redirect was asked for once");
    assert_eq!(real.requests(), 1, "and followed once");

    // And the digest still decides: the same redirect with the wrong digest is
    // refused rather than trusted because a server sent us there.
    let cache = CacheDir::new();
    let refused = with_cache_dir(&cache.path(), || {
        obtain_archive(
            &front.url(),
            "00000000000000000000000000000000",
            &aruna::job::Job::unattended(),
        )
    });
    assert!(
        matches!(refused, Err(ArunaError::ChecksumMismatch { .. })),
        "a redirected body escaped the digest check"
    );
}

/// A redirect that points at itself must end, and end quickly.
///
/// Following redirects without a limit is a hang: the client goes round for
/// ever, the run looks frozen and the only way out is killing it. The bound is
/// the HTTP client's, but a bound nobody checks is a bound that can be
/// configured away.
#[test]
fn a_redirect_loop_gives_up_instead_of_going_round_for_ever() {
    let looping = Origin::redirecting(None);
    let cache = CacheDir::new();

    // Timed inside the lock, not around it. `with_cache_dir` serialises the
    // tests that set the cache environment, and a neighbour waiting out its
    // retries would otherwise be counted here — which it was: ten seconds of
    // someone else's backoff, attributed to a redirect chain that takes
    // milliseconds.
    let (outcome, elapsed) = with_cache_dir(&cache.path(), || {
        let started = std::time::Instant::now();
        let outcome = obtain_archive(
            &looping.url(),
            &md5_hex(&body()),
            &aruna::job::Job::unattended(),
        );
        (outcome, started.elapsed())
    });

    // `let Err` rather than `expect_err`: `Archive` carries no `Debug`, and
    // giving it one to phrase an assertion would be the test deciding what the
    // type looks like.
    let Err(error) = outcome else {
        panic!("a redirect loop was followed to a result");
    };
    // What the client decided, said again to whoever is watching. `Failure`
    // called every network failure retryable, so the one transport failure this
    // client knows to be settled was the one a window would have offered
    // *Retry* for — 71 MiB fetched again to walk the identical chain.
    assert!(
        !aruna::app::Failure::of(&error).retryable,
        "a window was offered Retry for a redirect loop"
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "gave up only after {elapsed:?}"
    );
    // Walked once, not once per attempt. The client bounds the chain at five;
    // retrying a loop only walks the identical loop again, so it is not
    // retried. Before that arm existed this was 15 requests and eight seconds,
    // six of them asleep between attempts.
    assert!(
        looping.requests() <= 6,
        "the loop was walked {} times, so it is being retried",
        looping.requests()
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "took {elapsed:?}, which is long enough to have slept between retries"
    );
    assert!(
        cache.files().is_empty(),
        "a loop left something in the cache"
    );
}

/// A sink that keeps the Zenodo advisories and lets the rest go by.
#[derive(Default)]
struct Notices(std::sync::Mutex<Vec<String>>);

impl aruna::progress::Progress for Notices {
    fn report(&self, event: aruna::progress::Event<'_>) {
        if let aruna::progress::Event::ZenodoNotice { message } = event {
            self.0
                .lock()
                .expect("not poisoned")
                .push(message.to_string());
        }
    }
}

/// A newer edition of the corpus is announced, and the run goes ahead anyway.
///
/// The other tests here fill the release lookup with silence, which is what a
/// hermetic suite needs and is also the way to end up with a seam that only
/// ever says nothing: deleting the announcement from the download path would
/// break none of them. This one drives the same seam the other way — the
/// lookup reports that the corpus has moved on — and holds both halves of
/// `zenodo::advice`'s promise: the reader is told, and the pinned archive is
/// still what arrives.
#[test]
fn a_newer_edition_is_announced_and_the_pinned_archive_still_arrives() {
    fn superseded(_record_id: u64) -> aruna::error::Result<aruna::zenodo::Release> {
        Ok(aruna::zenodo::Release {
            record_id: aruna::ZENODO_RECORD + 1,
            file: "TLHbasisONLINE25_2_ZENODO_Beta_04.zip".to_string(),
            md5: None,
            published: Some("2026-09-01".to_string()),
        })
    }

    let payload = body();
    let origin = Origin::start(payload.clone());
    let cache = CacheDir::new();
    let notices = Notices::default();
    let cancel = aruna::job::Cancel::new();
    let job = aruna::job::Job::new(&notices, &cancel);

    let archive = with_cache_dir(&cache.path(), || {
        aruna::obtain_archive_advised_by(&origin.url(), &md5_hex(&payload), &job, superseded)
    })
    .expect("a corpus that has moved on is not a reason to refuse the pinned one");

    assert_eq!(
        std::fs::read(archive.path()).expect("read"),
        payload,
        "the advice was taken as an instruction"
    );

    let said = notices.0.lock().expect("not poisoned").clone();
    assert_eq!(said.len(), 1, "said {} times, not once", said.len());
    assert!(
        said[0].contains(&(aruna::ZENODO_RECORD + 1).to_string()),
        "the notice does not name the newer record: {}",
        said[0]
    );
}

/// The cache directory path is a regular file.
///
/// Section 9's "file where a directory is expected": creating the directory
/// fails, and that is a reason to do without a cache rather than to fail a run
/// that would otherwise succeed.
#[test]
fn a_file_where_the_cache_directory_belongs_does_not_fail_the_run() {
    let payload = body();
    let origin = Origin::start(payload.clone());
    let dir = tempdir().expect("tempdir");
    let occupied = dir.path().join("cache");
    std::fs::write(&occupied, b"not a directory").expect("write");

    let archive = with_cache_dir(&occupied, || {
        obtain_archive(
            &origin.url(),
            &md5_hex(&payload),
            &aruna::job::Job::unattended(),
        )
    })
    .expect("a file in the way must not fail the run");

    assert!(
        matches!(archive, Archive::Temporary(_)),
        "the archive was reported as cached into a file"
    );
    assert_eq!(std::fs::read(archive.path()).expect("read"), payload);
    // And the file that was in the way is untouched.
    assert_eq!(
        std::fs::read(&occupied).expect("read"),
        b"not a directory",
        "the file standing where the cache belongs was overwritten"
    );
}
