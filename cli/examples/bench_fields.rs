//! The parse alone, on real header windows, with the archive out of the way.
//!
//! ```text
//! cargo run --release --example bench_fields -- fixtures/…zip
//! ```
//!
//! Parsing is the second-largest stage of a run — 338 ms of 1222 — and until
//! now it could only be timed inside the pipeline, where it is inseparable from
//! inflating the ZIP. That makes it the one compute stage nobody could optimise
//! and check: any change would be measured against a number that is 72 %
//! something else.
//!
//! The archive is read once, as setup, and every header window is kept. What is
//! measured after that touches no file:
//!
//! * **windows** — cutting the two views a document is read through
//!   ([`Windows::of`]): finding the header block inside the leading bytes.
//! * **fields** — reading those windows into a record ([`parse_windows`]),
//!   which is where every extractor runs.
//! * **whole** — both, the way the pipeline calls it.
//!
//! Reported as min / p50 / p95 / max over `ARUNA_BENCH_RUNS` runs (default 7)
//! after `ARUNA_BENCH_WARMUP` (default 2) that are not recorded, plus the
//! throughput each stage works out to over the bytes it walked. Throughput is
//! the number that answers the open question about this stage: whether the
//! extractors are each walking the whole window again.

use aruna::parse::fields;
use aruna::parse::{parse_windows, HEADER_READ_LIMIT};
use aruna::parse::{ManuscriptRecord, Windows};
use std::env;
use std::hint::black_box;
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// One document as the pipeline hands it to the parse.
struct Document {
    path: String,
    text: String,
}

fn main() -> ExitCode {
    let Some(zip) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: bench_fields <archive.zip>");
        return ExitCode::FAILURE;
    };
    let runs = env_usize("ARUNA_BENCH_RUNS", 7);
    // Truncating the windows answers what the throughput alone cannot: whether
    // the cost follows the bytes (extractors walking the window repeatedly) or
    // the documents (per-record work that a shorter window does not avoid).
    let cap = env_usize("ARUNA_BENCH_WINDOW", HEADER_READ_LIMIT);
    let warmup = env_usize("ARUNA_BENCH_WARMUP", 2);

    // Setup: not part of any measurement below.
    eprintln!("reading header windows from {}…", zip.display());
    let documents = match collect(&zip) {
        Ok(documents) if !documents.is_empty() => documents,
        Ok(_) => {
            eprintln!("the archive yielded no manuscripts");
            return ExitCode::FAILURE;
        }
        Err(err) => {
            eprintln!("failed to read {}: {err}", zip.display());
            return ExitCode::FAILURE;
        }
    };
    let documents: Vec<Document> = documents
        .into_iter()
        .map(|d| Document {
            text: aruna::parse::truncate_on_char_boundary(&d.text, cap).to_string(),
            path: d.path,
        })
        .collect();
    let window_bytes: usize = documents.iter().map(|d| d.text.len()).sum();

    let mut cutting = Vec::with_capacity(runs);
    let mut reading = Vec::with_capacity(runs);
    let mut whole = Vec::with_capacity(runs);

    for run in 1..=(warmup + runs) {
        // Cutting the windows, without reading them.
        let start = Instant::now();
        for document in &documents {
            black_box(Windows::of(&document.text));
        }
        let cut = start.elapsed();

        // Reading windows that are already cut. Prepared first so the cutting
        // is not counted twice.
        let prepared: Vec<Windows<'_>> = documents.iter().map(|d| Windows::of(&d.text)).collect();
        let start = Instant::now();
        for (document, windows) in documents.iter().zip(&prepared) {
            let record: ManuscriptRecord = parse_windows(&document.path, *windows);
            black_box(&record);
        }
        let read = start.elapsed();

        let start = Instant::now();
        for document in &documents {
            let record = aruna::parse::parse_manuscript(&document.path, &document.text);
            black_box(&record);
        }
        let both = start.elapsed();

        if run > warmup {
            cutting.push(cut);
            reading.push(read);
            whole.push(both);
        }
    }

    let mib = window_bytes as f64 / (1024.0 * 1024.0);
    println!();
    println!("archive:     {}", zip.display());
    println!(
        "documents:   {} ({:.1} MiB of windows, {:.1} KiB each on average)",
        documents.len(),
        mib,
        window_bytes as f64 / documents.len() as f64 / 1024.0
    );
    println!("runs:        {runs} (after {warmup} warm-up)");
    println!();
    println!(
        "{:<10} {:>9} {:>9} {:>9} {:>9} {:>12}",
        "stage", "min", "p50", "p95", "max", "throughput"
    );
    println!("{}", "-".repeat(62));
    report("windows", &mut cutting, mib);
    report("fields", &mut reading, mib);
    report("whole", &mut whole, mib);
    println!("{}", "-".repeat(62));

    per_field(&documents, runs, warmup, mib);
    println!(
        "Throughput is over the window bytes walked, not the archive. A stage \
         reading each window once\nwould land near memory speed; well under it \
         means the window is being walked more than once."
    );

    ExitCode::SUCCESS
}

/// What each extractor costs, in the order and under the conditions
/// `parse_windows` actually runs them.
///
/// The year fallback is only consulted when the editor did not carry one, and
/// timing it over every document would report a cost the program never pays —
/// so the condition is reproduced rather than flattened. The sum lands near the
/// `fields` figure above, which is the check that nothing here is measuring a
/// different shape from the parse.
fn per_field(documents: &[Document], runs: usize, warmup: usize, mib: f64) {
    const NAMES: [&str; 7] = [
        "sigla",
        "cth",
        "editor+year",
        "year fallback",
        "inv",
        "lang",
        "corpus",
    ];
    let mut times: [Vec<Duration>; 7] = Default::default();
    let prepared: Vec<Windows<'_>> = documents.iter().map(|d| Windows::of(&d.text)).collect();

    for run in 1..=(warmup + runs) {
        let mut round = [Duration::ZERO; 7];

        for (document, w) in documents.iter().zip(&prepared) {
            let (header, window, path) = (w.header, w.window, document.path.as_str());

            let start = Instant::now();
            let sigla = fields::extract_sigla(header, window, path);
            round[0] += start.elapsed();
            black_box(&sigla);

            let start = Instant::now();
            let cth = fields::extract_cth(header, window, path);
            round[1] += start.elapsed();
            black_box(&cth);

            let start = Instant::now();
            let (editor, year) = fields::extract_editor_and_year(header);
            round[2] += start.elapsed();
            black_box(&editor);

            // Only when the editor did not carry one, which is what the parse does.
            if year.is_none() {
                let start = Instant::now();
                let fallback = fields::extract_year_fallback(header, window);
                round[3] += start.elapsed();
                black_box(&fallback);
            }

            let start = Instant::now();
            let inv = fields::extract_inv(header, window);
            round[4] += start.elapsed();
            black_box(&inv);

            let start = Instant::now();
            let lang = fields::extract_lang(window);
            round[5] += start.elapsed();
            black_box(&lang);

            let start = Instant::now();
            let corpus = fields::extract_corpus(path);
            round[6] += start.elapsed();
            black_box(&corpus);
        }

        if run > warmup {
            for (slot, measured) in times.iter_mut().zip(round) {
                slot.push(measured);
            }
        }
    }

    println!();
    println!("per extractor, in the order the parse runs them:");
    println!("{}", "-".repeat(62));
    let mut total = Duration::ZERO;
    for (name, samples) in NAMES.iter().zip(times.iter_mut()) {
        if let Some(median) = median(samples) {
            total += median;
        }
        report(name, samples, mib);
    }
    println!("{}", "-".repeat(62));
    println!(
        "sum of medians: {:.1}ms — compare with `fields` above; a large gap \n\
         would mean this breakdown is measuring a different shape.",
        total.as_secs_f64() * 1000.0
    );
}

fn median(samples: &mut [Duration]) -> Option<Duration> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_unstable();
    Some(samples[samples.len() / 2])
}

/// Every manuscript's header window, through the corpus's own gates.
///
/// The same bytes `archive::parse_zip` hands the parse, so the benchmark
/// measures the shape the program actually meets.
fn collect(zip: &std::path::Path) -> std::io::Result<Vec<Document>> {
    use aruna::parse::{is_manuscript_xml, looks_like_manuscript};

    let file = std::fs::File::open(zip)?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::with_capacity(256 * 1024, file))
        .map_err(std::io::Error::other)?;

    let mut out = Vec::new();
    let mut window = Vec::with_capacity(HEADER_READ_LIMIT);
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(std::io::Error::other)?;
        let path = entry.name().to_string();
        if !is_manuscript_xml(&path) {
            continue;
        }
        window.clear();
        entry
            .by_ref()
            .take(HEADER_READ_LIMIT as u64)
            .read_to_end(&mut window)?;
        let text = String::from_utf8_lossy(&window).into_owned();
        if !looks_like_manuscript(&text) {
            continue;
        }
        out.push(Document { path, text });
    }
    Ok(out)
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn report(label: &str, samples: &mut [Duration], mib: f64) {
    if samples.is_empty() {
        return;
    }
    samples.sort_unstable();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |q: f64| samples[quantile_index(samples.len(), q)];
    let median = at(0.50);
    println!(
        "{label:<10} {:>8.1}ms {:>8.1}ms {:>8.1}ms {:>8.1}ms {:>7.0} MiB/s",
        ms(samples[0]),
        ms(median),
        ms(at(0.95)),
        ms(samples[samples.len() - 1]),
        mib / median.as_secs_f64(),
    );
}

/// Nearest-rank: with seven runs an interpolated p95 would be inventing
/// precision the sample does not carry.
fn quantile_index(len: usize, q: f64) -> usize {
    ((q * len as f64).ceil() as usize)
        .saturating_sub(1)
        .min(len - 1)
}
