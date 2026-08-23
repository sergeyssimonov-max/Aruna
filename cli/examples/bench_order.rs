//! Sorting alone, on the real inventory, apart from what it took to get it.
//!
//! ```text
//! cargo run --release --example bench_order -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
//! ```
//!
//! `bench_parse` reports the pipeline as it runs, which means sorting is timed
//! once per archive read — a second of inflating for six milliseconds of the
//! thing being measured. This reads the archive once, keeps the records, and
//! then sorts a fresh copy of them as many times as asked.
//!
//! The copy is deliberate and is not counted: [`sort_records`] permutes in
//! place, so sorting the same slice twice would measure sorting something
//! already sorted, which is not the work the program does. Cloning is timed
//! separately and reported, so the two costs can be told apart rather than
//! silently added.
//!
//! Runs are reported as min / median / max over `ARUNA_BENCH_RUNS` (default 20),
//! after `ARUNA_BENCH_WARMUP` (default 3) unrecorded runs — the first sort of a
//! process pays for page faults on memory nothing has touched yet, and that is
//! a different question from how long sorting takes once it is warm.
//!
//! Deliberately dependency-free: the numbers only need to be good enough to
//! decide whether an optimisation earns its complexity.

use aruna::archive::parse_zip;
use aruna::order::sort_records;
use aruna::parse::ManuscriptRecord;
use std::env;
use std::hint::black_box;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let Some(zip) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: bench_order <archive.zip>");
        return ExitCode::FAILURE;
    };
    if !zip.is_file() {
        eprintln!("no such archive: {}", zip.display());
        return ExitCode::FAILURE;
    }

    let runs = env_usize("ARUNA_BENCH_RUNS", 20);
    let warmup = env_usize("ARUNA_BENCH_WARMUP", 3);

    // Setup, and not part of any measurement below.
    eprintln!("reading {} …", zip.display());
    let sorted = match parse_zip(&zip, &aruna::job::Job::unattended()) {
        Ok(records) => records,
        Err(err) => {
            eprintln!("failed to read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };

    // The records come back in display order, and sorting an already-sorted
    // slice is the one input the program never has. Reversing costs nothing to
    // do and gives every run the same starting arrangement, so the runs are
    // comparable with each other as well as with a later build.
    let unsorted: Vec<ManuscriptRecord> = sorted.iter().rev().cloned().collect();

    let mut clone_times = Vec::with_capacity(runs);
    let mut sort_times = Vec::with_capacity(runs);

    for run in 1..=(warmup + runs) {
        let start = Instant::now();
        let mut records = unsorted.clone();
        let cloned = start.elapsed();

        let start = Instant::now();
        sort_records(&mut records);
        let elapsed = start.elapsed();

        // Without this the sorted records are never read and the whole call is
        // fair game for the optimiser.
        black_box(&records);

        if run > warmup {
            clone_times.push(cloned);
            sort_times.push(elapsed);
        }
    }

    println!();
    println!("archive:     {}", zip.display());
    println!("manuscripts: {}", sorted.len());
    println!("runs:        {runs} (after {warmup} warm-up)");
    println!();
    println!(
        "{:<14} {:>10} {:>10} {:>10}",
        "step", "min", "median", "max"
    );
    println!("{}", "-".repeat(48));
    report("clone (setup)", &mut clone_times);
    report("sort", &mut sort_times);
    println!("{}", "-".repeat(48));

    ExitCode::SUCCESS
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn report(label: &str, samples: &mut [Duration]) {
    if samples.is_empty() {
        println!("{label:<14} {:>10}", "no runs");
        return;
    }
    samples.sort_unstable();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "{label:<14} {:>9.2}ms {:>9.2}ms {:>9.2}ms",
        ms(samples[0]),
        ms(samples[samples.len() / 2]),
        ms(samples[samples.len() - 1]),
    );
}
