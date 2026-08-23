//! One cache directory, several runs at once.
//!
//! `cache_lifecycle.rs` drives the cache one caller at a time, and
//! `cli_process.rs::two_runs_at_once_do_not_interfere` runs two binaries
//! together — but with `ARUNA_ZIP` set, which skips the download and the cache
//! entirely. So the path where two runs *compete* — both miss, both fetch 71
//! MiB, both try to land it under the same name in the same directory — had
//! nothing on it at all.
//!
//! That path is not hypothetical. The cache is a shared directory under the
//! user's home; two terminals, a shell loop, or a rebuild kicked off twice is
//! all it takes. What must hold is that the loser of the race is not left with
//! a truncated archive, that the directory does not fill with half-downloads,
//! and that the second wave costs nothing.
//!
//! Everything here is in-process rather than in child processes for one
//! reason: the binary downloads from the pinned Zenodo URL and cannot be
//! pointed anywhere else, so a child cannot be made to talk to a local server.
//! `obtain_archive` can, and it is the function that owns this decision.

mod support;

use aruna::cache::Archive;
use aruna::job::Job;
use aruna::md5::md5_hex;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime};
use support::{obtain_archive, open_descriptors, Origin};
use tempfile::{tempdir, TempDir};

/// The cache location is chosen from a process-wide environment variable, so
/// two tests in this binary cannot each have their own at the same time.
///
/// Serialised rather than merged into one giant test: each of these is a
/// separate property and should fail separately. The lock is held for the whole
/// of each test, and no other test binary sets this variable, so nothing else
/// in the run can see the value while it is set.
fn cache_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A cache directory nobody else is using, pointed at by the variable
/// `cache::cache_dir` reads.
fn cache_dir() -> (TempDir, PathBuf) {
    let dir = tempdir().expect("tempdir");
    let cache = dir.path().join("cache");
    std::fs::create_dir_all(&cache).expect("cache dir");
    // SAFETY-ish: the guard from `cache_lock` is held by every caller, and no
    // other test binary reads or writes this variable.
    std::env::set_var(aruna::cache::CACHE_DIR_ENV, &cache);
    std::env::remove_var("XDG_CACHE_HOME");
    (dir, cache)
}

/// An archive big enough that two downloads of it genuinely overlap rather than
/// completing inside one scheduler slice.
fn body() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(512 * 1024);
    let mut seed = 0x9e37_79b9_u32;
    while bytes.len() < 512 * 1024 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        bytes.extend_from_slice(&seed.to_le_bytes());
    }
    bytes
}

/// Everything in `dir`, by name.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read cache")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Six runs racing for one archive all end up with the archive.
///
/// Not "one wins": every caller must be handed a file that hashes to what it
/// asked for. The landing is a rename over a complete file, so a run that
/// arrives while another is mid-download either downloads its own copy or finds
/// the finished one — never the half.
#[test]
fn runs_racing_for_one_archive_all_get_a_whole_one() {
    let _guard = cache_lock();
    let (_dir, cache) = cache_dir();
    let payload = body();
    let digest = md5_hex(&payload);
    // Gated on six: every one of them misses an empty cache, so every one
    // reaches the network, and none is answered until all six are there. The
    // overlap is then a property of the test rather than of the machine.
    let origin = Origin::gated(payload.clone(), 6);
    let url = origin.url();

    let outcomes: Vec<PathBuf> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| {
                let url = url.clone();
                let digest = digest.clone();
                scope.spawn(move || {
                    obtain_archive(&url, &digest, &Job::unattended())
                        .expect("a racing run still gets it")
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| match h.join().expect("no thread panicked") {
                Archive::Cached(path) | Archive::Temporary(path) => path,
            })
            .collect()
    });

    assert_eq!(outcomes.len(), 6);
    for path in &outcomes {
        let bytes = std::fs::read(path).expect("the archive each run was handed");
        assert_eq!(
            md5_hex(&bytes),
            digest,
            "{} does not hash to what the caller asked for",
            path.display()
        );
    }

    // They were genuinely concurrent — otherwise this test proves nothing about
    // a race it never produced.
    assert!(
        origin.concurrent_peak() > 1,
        "the six runs were served one after another; the race under test never happened"
    );

    // One name, one file. The digest is in the name, so six runs of one archive
    // cannot produce six files — and a `.part` left behind would be one.
    assert_eq!(
        entries(&cache).len(),
        1,
        "six runs left {:?} in the cache",
        entries(&cache)
    );
    assert!(
        entries(&cache)[0].contains(&digest),
        "the file in the cache is not named for the digest it holds: {:?}",
        entries(&cache)
    );

    // Bounded, not exact: without a lock between processes, several may
    // legitimately fetch. What must not happen is retries piling on top.
    assert!(
        (1..=6).contains(&origin.requests()),
        "six racing runs made {} requests; that is more than one each",
        origin.requests()
    );
}

/// Once one has landed, the rest cost nothing.
///
/// The whole reason the cache exists. A second wave that reached the network at
/// all would mean the landing is not visible to the next caller, which is the
/// failure mode a rename into place is supposed to rule out.
#[test]
fn a_second_wave_is_served_entirely_from_the_cache() {
    let _guard = cache_lock();
    let (_dir, cache) = cache_dir();
    let payload = body();
    let digest = md5_hex(&payload);
    let origin = Origin::serving(payload);
    let url = origin.url();

    obtain_archive(&url, &digest, &Job::unattended()).expect("the cold run downloads");
    let after_cold = origin.requests();
    assert_eq!(after_cold, 1, "the cold run made {after_cold} requests");

    std::thread::scope(|scope| {
        for _ in 0..6 {
            let url = url.clone();
            let digest = digest.clone();
            scope.spawn(move || {
                obtain_archive(&url, &digest, &Job::unattended()).expect("a warm run is served");
            });
        }
    });

    assert_eq!(
        origin.requests(),
        after_cold,
        "a warm wave went back to the network"
    );
    assert_eq!(entries(&cache).len(), 1);
}

/// No connection outlives the run that opened it, and no descriptor either.
///
/// The export already has a leak test; the network path had none, and it is the
/// one that holds sockets. Measured across waves rather than once, because a
/// single reading cannot tell a descriptor that is still in use from one that
/// was never given back.
#[test]
fn repeated_downloads_do_not_accumulate_connections_or_descriptors() {
    let _guard = cache_lock();
    let (_dir, cache) = cache_dir();
    let payload = body();
    let origin = Origin::serving(payload.clone());
    let url = origin.url();

    // Each wave asks for a different digest so every one is a real download
    // rather than a cache hit — the point is the sockets, not the cache.
    let wave = |n: usize| {
        let mut distinct = payload.clone();
        distinct.extend_from_slice(&n.to_le_bytes());
        let digest = md5_hex(&distinct);
        // The server serves `payload`, so this download is refused on its
        // digest. Refused or accepted, the socket must be given back.
        let _ = obtain_archive(&format!("{url}?wave={n}"), &digest, &Job::unattended());
    };

    // Warm-up: the first run through pulls in TLS state, DNS caches and lazy
    // statics that a later reading would otherwise charge to a leak.
    for n in 0..3 {
        wave(n);
    }
    let before = open_descriptors();

    for n in 3..15 {
        wave(n);
    }
    let after = open_descriptors();

    assert_eq!(
        origin.live_connections(),
        0,
        "the server still has connections open after every client finished"
    );
    assert!(
        after <= before + 2,
        "twelve downloads left descriptors behind: {before} before, {after} after"
    );
    // A refused download keeps nothing: the cache holds no archive and no
    // scratch file from any of the fifteen.
    assert!(
        entries(&cache).is_empty(),
        "refused downloads left {:?} in the cache",
        entries(&cache)
    );
}

/// An abandoned download is swept up; one that may still be in flight is not.
///
/// `sweep_unfinished` is what keeps a killed run from costing the cache a
/// permanent 10 MiB of nothing, and the twenty-four hours it waits is what
/// keeps it from deleting the scratch file of a run happening right now. Both
/// halves matter and neither was tested: a sweep that was too eager would make
/// the race above corrupt rather than merely wasteful.
#[test]
fn a_stale_part_file_is_swept_and_a_fresh_one_is_left_alone() {
    let _guard = cache_lock();
    let (_dir, cache) = cache_dir();
    let payload = body();
    let digest = md5_hex(&payload);
    let origin = Origin::serving(payload);

    let stale = cache.join("TLHbasis.deadbeefdeadbeefdeadbeefdeadbeef.zip.99.part");
    let fresh = cache.join("TLHbasis.cafebabecafebabecafebabecafebabe.zip.98.part");
    std::fs::write(&stale, b"half an archive from a run that was killed").expect("stale");
    std::fs::write(&fresh, b"half an archive from a run still going").expect("fresh");

    // Two days back: past the day `ABANDONED_AFTER` waits, and reached through
    // the file's own timestamps rather than by sleeping.
    let long_ago = SystemTime::now() - Duration::from_secs(48 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&stale)
        .expect("open stale")
        .set_times(std::fs::FileTimes::new().set_modified(long_ago))
        .expect("backdate");

    obtain_archive(&origin.url(), &digest, &Job::unattended()).expect("the run downloads");

    assert!(
        !stale.exists(),
        "an abandoned download from two days ago was left in the cache"
    );
    assert!(
        fresh.exists(),
        "a scratch file young enough to belong to a running download was deleted — \
         a concurrent run would have lost its archive"
    );
}
