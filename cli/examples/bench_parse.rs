//! Stage timings for the CLI pipeline on a real TLHdig archive.
//!
//! ```text
//! cargo run --release --example bench_parse -- fixtures/TLHbasisONLINE25_1_ZENODO_Beta_03.zip
//! ```
//!
//! Prints min / median / max per stage over `ARUNA_BENCH_RUNS` runs (default 5).
//! Peak memory is not measured here — wrap the command in `/usr/bin/time -l`
//! (macOS) or `/usr/bin/time -v` (Linux) for that.
//!
//! Deliberately dependency-free: the numbers only need to be good enough to
//! decide whether an optimisation earns its complexity.

use aruna::archive::parse_zip_timed;
use aruna::html::render_html;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

fn main() -> ExitCode {
    let Some(zip) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: bench_parse <archive.zip>");
        return ExitCode::FAILURE;
    };
    if !zip.is_file() {
        eprintln!("no such archive: {}", zip.display());
        return ExitCode::FAILURE;
    }

    let runs: usize = env::var("ARUNA_BENCH_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let mut read = Vec::with_capacity(runs);
    let mut parse = Vec::with_capacity(runs);
    let mut render = Vec::with_capacity(runs);
    let mut manuscripts = 0;
    let mut html_bytes = 0;

    for run in 1..=runs {
        // The pipeline reads, parses and sorts in one pass — an entry is
        // finished with before the next is read — so the stages are timed
        // inside it rather than around it.
        let (records, times) = match parse_zip_timed(&zip) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!("failed to read {}: {err}", zip.display());
                return ExitCode::FAILURE;
            }
        };
        read.push(times.inflate);
        parse.push(times.parse);

        let start = Instant::now();
        let html = render_html(&records, "bench", "bench");
        render.push(start.elapsed());

        manuscripts = records.len();
        html_bytes = html.len();
        eprintln!("run {run}/{runs} done");
    }

    println!();
    println!("archive:     {}", zip.display());
    println!("manuscripts: {manuscripts}");
    println!("html:        {} KiB", html_bytes / 1024);
    println!("runs:        {runs}");
    println!();
    println!("{:<14} {:>10} {:>10} {:>10}", "stage", "min", "median", "max");
    println!("{}", "-".repeat(48));
    report("read (zip)", &mut read);
    report("parse + sort", &mut parse);
    report("render html", &mut render);
    println!("{}", "-".repeat(48));

    let total: Duration = [&read, &parse, &render]
        .into_iter()
        .filter_map(|stage| stage.iter().min())
        .sum();
    println!("{:<14} {:>9.1}ms", "total (min)", total.as_secs_f64() * 1000.0);

    ExitCode::SUCCESS
}

fn report(label: &str, samples: &mut [Duration]) {
    samples.sort_unstable();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    println!(
        "{label:<14} {:>9.1}ms {:>9.1}ms {:>9.1}ms",
        ms(samples[0]),
        ms(samples[samples.len() / 2]),
        ms(samples[samples.len() - 1]),
    );
}
