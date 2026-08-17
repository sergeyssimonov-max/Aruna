//! MD5 throughput, which is what a warm run mostly spends its time on.
//!
//! ```text
//! cargo run --release --example bench_digest
//! cargo run --release --example bench_digest -- fixtures/…zip
//! ```
//!
//! Every run that finds the archive in the cache re-hashes all 71 MiB of it
//! before trusting it — that check is the price of not serving a corrupted
//! archive, and it is not negotiable, so the only way to make a warm start
//! cheaper is to make the digest faster.
//!
//! Two numbers, deliberately apart:
//!
//! * **memory** — the same bytes hashed out of RAM, repeatedly. This is the
//!   compression function and nothing else, which is the thing an optimisation
//!   here would change.
//! * **file** — the archive read from disk and hashed, which is what
//!   `cache::lookup` actually does. It includes the read, so it moves with the
//!   page cache and the disk as well as with the code.
//!
//! With no argument the memory figure is measured against a pseudo-random
//! buffer of the archive's size, so the benchmark runs anywhere; MD5's cost
//! depends on the length of its input and not on the values in it.
//!
//! Reported as min / p50 / p95 / max over `ARUNA_BENCH_RUNS` runs (default 15),
//! after `ARUNA_BENCH_WARMUP` (default 2) that are not recorded. Tail figures
//! are the point: a median hides the pause that a user actually notices.
//!
//! Deliberately dependency-free, like the rest of the benchmarks here.

use aruna::md5::Md5;
use std::env;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// The archive this program exists to hash, to the nearest MiB.
const ARCHIVE_BYTES: usize = 71 * 1024 * 1024;

fn main() -> ExitCode {
    let runs = env_usize("ARUNA_BENCH_RUNS", 15);
    let warmup = env_usize("ARUNA_BENCH_WARMUP", 2);
    let archive = env::args_os().nth(1).map(PathBuf::from);

    // Setup, and not part of any measurement below.
    let bytes = match &archive {
        Some(path) => match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("cannot read {}: {err}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => pseudo_random(ARCHIVE_BYTES),
    };

    println!();
    match &archive {
        Some(path) => println!("input:  {} ({} MiB)", path.display(), bytes.len() >> 20),
        None => println!("input:  generated buffer ({} MiB)", bytes.len() >> 20),
    }
    println!("runs:   {runs} (after {warmup} warm-up)");
    println!();
    println!(
        "{:<10} {:>9} {:>9} {:>9} {:>9} {:>10}",
        "source", "min", "p50", "p95", "max", "median"
    );
    println!("{}", "-".repeat(60));

    let mut memory = Vec::with_capacity(runs);
    for run in 1..=(warmup + runs) {
        let start = Instant::now();
        let mut digest = Md5::new();
        digest.update(&bytes);
        let hex = digest.finish_hex();
        let elapsed = start.elapsed();
        // Without this the digest is never read and the whole call can go.
        black_box(&hex);
        if run > warmup {
            memory.push(elapsed);
        }
    }
    report("memory", &mut memory, bytes.len());

    // The file figure needs a file; with none given there is nothing honest to
    // print, so nothing is printed.
    if let Some(path) = &archive {
        let mut from_file = Vec::with_capacity(runs);
        for run in 1..=(warmup + runs) {
            let start = Instant::now();
            let Ok(read) = fs::read(path) else {
                eprintln!("cannot re-read {}", path.display());
                return ExitCode::FAILURE;
            };
            let mut digest = Md5::new();
            digest.update(&read);
            let hex = digest.finish_hex();
            let elapsed = start.elapsed();
            black_box(&hex);
            if run > warmup {
                from_file.push(elapsed);
            }
        }
        report("file", &mut from_file, bytes.len());
    }

    println!("{}", "-".repeat(60));
    ExitCode::SUCCESS
}

/// Bytes that are not all the same, cheaply and without a dependency.
///
/// A buffer of zeroes would be hashed at exactly the same speed — MD5 does not
/// look at what the bytes are — but it would also be a buffer the allocator can
/// hand over without touching, which is not the shape a real read has.
fn pseudo_random(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// min / p50 / p95 / max, and the throughput the median works out to.
///
/// Nearest-rank quantiles: with fifteen runs an interpolated p95 would be
/// inventing precision the sample does not carry.
fn report(label: &str, samples: &mut [Duration], bytes: usize) {
    if samples.is_empty() {
        return;
    }
    samples.sort_unstable();
    let at = |q: f64| samples[quantile_index(samples.len(), q)];
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;

    let median = at(0.50);
    let mib_per_s = (bytes as f64 / (1024.0 * 1024.0)) / median.as_secs_f64();
    println!(
        "{label:<10} {:>8.1}ms {:>8.1}ms {:>8.1}ms {:>8.1}ms {:>7.0} MiB/s",
        ms(samples[0]),
        ms(median),
        ms(at(0.95)),
        ms(samples[samples.len() - 1]),
        mib_per_s,
    );
}

/// Nearest-rank index for quantile `q` over `len` sorted samples.
fn quantile_index(len: usize, q: f64) -> usize {
    let rank = (q * len as f64).ceil() as usize;
    rank.saturating_sub(1).min(len - 1)
}
