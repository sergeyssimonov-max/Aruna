//! The export, stage by stage, on a real archive.
//!
//! ```text
//! cargo run --release --example bench_export -- fixtures/…zip
//! ```
//!
//! Four numbers, and they are apart on purpose because they are limited by
//! different things:
//!
//! * **headers** — pass 1: inflating and parsing the first 16 KiB of every
//!   entry. Bounded by the ZIP, like the CLI's own run.
//! * **place** — deciding where 24 000 documents go. Pure, in memory, and the
//!   part an algorithm change would touch.
//! * **inventory** — rendering the page. Pure, in memory.
//! * **normalise** — the transform alone, over documents already read, with the
//!   reading not counted. This is what the second pass spends its CPU on.
//! * **build** — the whole thing including writing 372 MB to disk, which is
//!   what a user waits for.
//!
//! The pure stages are repeated `ARUNA_BENCH_RUNS` times (default 5) after
//! `ARUNA_BENCH_WARMUP` unrecorded runs; the two that touch the disk run once,
//! because repeating them measures the page cache more than the program.
//! Reported as min / p50 / max — a median alone hides the pause a user notices.
//!
//! Deliberately dependency-free, like the rest of the benchmarks here.

use aruna::export::{self, Fragment};
use aruna::order::sort_by_display_order;
use aruna::parse::ManuscriptRecord;
use std::env;
use std::hint::black_box;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let Some(zip) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: bench_export <archive.zip>");
        return ExitCode::FAILURE;
    };
    if !zip.is_file() {
        eprintln!("no such archive: {}", zip.display());
        return ExitCode::FAILURE;
    }
    let runs = env_usize("ARUNA_BENCH_RUNS", 5);
    let warmup = env_usize("ARUNA_BENCH_WARMUP", 1);

    // Pass 1, timed once: it is the archive that bounds it, and reading the
    // archive five times measures the page cache.
    eprintln!("reading headers…");
    let start = Instant::now();
    let mut fragments = match export::collect_fragments(&zip) {
        Ok(fragments) => fragments,
        Err(err) => {
            eprintln!("failed to read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };
    let headers = start.elapsed();
    sort_by_display_order(&mut fragments, |f: &Fragment| &f.record);
    let records: Vec<ManuscriptRecord> = fragments.iter().map(|f| f.record.clone()).collect();

    // Setup for the pure stages; none of this is counted below.
    let placed = match export::place(&fragments) {
        Ok(placed) => placed,
        Err(err) => {
            eprintln!("placement failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("sampling documents for the normaliser…");
    let documents = sample_documents(&zip, &fragments, 2_000);
    let sampled: usize = documents.iter().map(Vec::len).sum();

    let mut place_times = Vec::with_capacity(runs);
    let mut inventory_times = Vec::with_capacity(runs);
    let mut normalise_times = Vec::with_capacity(runs);

    for run in 1..=(warmup + runs) {
        let start = Instant::now();
        let fresh = export::place(&fragments).expect("placement is deterministic");
        let placing = start.elapsed();
        black_box(&fresh);

        let start = Instant::now();
        let html = export::render_inventory(&records, &placed, "bench");
        let rendering = start.elapsed();
        black_box(&html);

        // One buffer for all of them, which is how the export uses it.
        let mut out = Vec::new();
        let start = Instant::now();
        for document in &documents {
            out.clear();
            export::normalize_into(document, &mut out);
            black_box(&out);
        }
        let normalising = start.elapsed();

        if run > warmup {
            place_times.push(placing);
            inventory_times.push(rendering);
            normalise_times.push(normalising);
        }
    }

    // The whole build, once, into a directory that is removed afterwards.
    eprintln!("building a package…");
    let scratch = std::env::temp_dir().join(format!("aruna-bench-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&scratch);
    let start = Instant::now();
    let built = export::build(&zip, &scratch, "bench", &aruna::job::Job::unattended());
    let whole = start.elapsed();
    let documents_written = match &built {
        Ok(built) => built.documents,
        Err(err) => {
            eprintln!("build failed: {err}");
            let _ = std::fs::remove_dir_all(&scratch);
            return ExitCode::FAILURE;
        }
    };
    let _ = std::fs::remove_dir_all(&scratch);

    println!();
    println!("archive:     {}", zip.display());
    println!("manuscripts: {}", fragments.len());
    println!("runs:        {runs} (after {warmup} warm-up) for the in-memory stages");
    println!();
    println!("{:<22} {:>10} {:>10} {:>10}", "stage", "min", "p50", "max");
    println!("{}", "-".repeat(56));
    println!(
        "{:<22} {:>9.1}ms {:>10} {:>10}   (once)",
        "headers (pass 1)",
        ms(headers),
        "—",
        "—"
    );
    report("place", &mut place_times);
    report("inventory", &mut inventory_times);
    report(
        &format!("normalise ({} docs)", documents.len()),
        &mut normalise_times,
    );
    println!(
        "{:<22} {:>9.1}ms {:>10} {:>10}   (once, {} documents written)",
        "build (whole)",
        ms(whole),
        "—",
        "—",
        documents_written
    );
    println!("{}", "-".repeat(56));
    if let Some(median) = median(&normalise_times) {
        let mib = sampled as f64 / (1024.0 * 1024.0);
        println!(
            "normaliser throughput: {:.0} MiB/s over {:.1} MiB of documents",
            mib / median.as_secs_f64(),
            mib
        );
    }

    ExitCode::SUCCESS
}

/// Read up to `limit` documents out of the archive, for the normaliser to chew
/// on without the reading being part of the measurement.
fn sample_documents(zip: &std::path::Path, fragments: &[Fragment], limit: usize) -> Vec<Vec<u8>> {
    use std::collections::HashSet;
    use std::io::Read;

    let step = (fragments.len() / limit.max(1)).max(1);
    let wanted: HashSet<&str> = fragments
        .iter()
        .step_by(step)
        .map(|f| f.source.as_str())
        .collect();

    let Ok(file) = std::fs::File::open(zip) else {
        return Vec::new();
    };
    let Ok(mut archive) = zip::ZipArchive::new(std::io::BufReader::new(file)) else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(wanted.len());
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        if !wanted.contains(entry.name()) {
            continue;
        }
        let mut bytes = Vec::new();
        if entry.read_to_end(&mut bytes).is_ok() {
            out.push(bytes);
        }
    }
    out
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn median(samples: &[Duration]) -> Option<Duration> {
    samples.get(samples.len() / 2).copied()
}

fn report(label: &str, samples: &mut [Duration]) {
    if samples.is_empty() {
        return;
    }
    samples.sort_unstable();
    println!(
        "{label:<22} {:>9.1}ms {:>9.1}ms {:>9.1}ms",
        ms(samples[0]),
        ms(samples[samples.len() / 2]),
        ms(samples[samples.len() - 1]),
    );
}
