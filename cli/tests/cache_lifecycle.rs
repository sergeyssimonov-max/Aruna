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

use aruna::cache::{self, Archive};
use aruna::error::ArunaError;
use aruna::md5::md5_hex;
use aruna::obtain_archive;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
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
}

impl Origin {
    fn start(body: Vec<u8>) -> Self {
        // Port 0: the operating system picks a free one, so concurrent tests
        // cannot collide over a hard-coded number.
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);

        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                counter.fetch_add(1, Ordering::SeqCst);
                Self::answer(&mut stream, &body);
            }
        });

        Origin { port, hits }
    }

    fn answer(stream: &mut TcpStream, body: &[u8]) {
        // Read the request line so the client is not writing into a socket
        // nobody is reading.
        let mut scratch = [0u8; 1024];
        let _ = stream.read(&mut scratch);
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
        let first = obtain_archive(&origin.url(), &digest).expect("cold run downloads");
        let second = obtain_archive(&origin.url(), &digest).expect("warm run is served");
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
        obtain_archive(&origin.url(), "00000000000000000000000000000000")
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
        obtain_archive(&origin.url(), &md5_hex(&first_payload)).expect("first edition");
        let before = std::fs::read_dir(cache.path()).expect("read").count();

        // The same file name at the source, a different digest: what a
        // republished record looks like.
        let url = format!("http://127.0.0.1:{}/TLHbasis.zip", later.port);
        obtain_archive(&url, &md5_hex(&second_payload)).expect("second edition");
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
        obtain_archive(&origin.url(), &md5_hex(&payload))
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
        obtain_archive(&format!("http://127.0.0.1:{port}/x.zip"), &md5_hex(&body()))
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
