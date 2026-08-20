//! Scratch: what the font contract costs on the real corpus.
//!
//! `observe` runs once per document over its whole text, and the text is the
//! whole corpus — some 390 MB. Whatever it does per character, it does about
//! four hundred million times, so it is worth knowing the number before
//! changing anything about it.
use aruna::export::manifest::FontContract;
use aruna::parse::{
    is_manuscript_xml, looks_like_manuscript, truncate_on_char_boundary, HEADER_READ_LIMIT,
};
use std::io::Read;
use std::time::Instant;

fn main() {
    let Some(zip) = std::env::args_os().nth(1) else {
        eprintln!("usage: bench_fonts <archive.zip>");
        return;
    };
    let file = std::fs::File::open(&zip).expect("open");
    let mut archive =
        zip::ZipArchive::new(std::io::BufReader::with_capacity(1 << 18, file)).expect("zip");

    // Read everything first, so the measurement is the counting and not the
    // unzipping.
    let mut texts: Vec<String> = Vec::new();
    let mut bytes = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        if !is_manuscript_xml(entry.name()) {
            continue;
        }
        bytes.clear();
        entry.read_to_end(&mut bytes).expect("read");
        let text = String::from_utf8_lossy(&bytes);
        if !looks_like_manuscript(truncate_on_char_boundary(&text, HEADER_READ_LIMIT)) {
            continue;
        }
        texts.push(text.into_owned());
    }
    let chars: usize = texts.iter().map(|t| t.chars().count()).sum();
    println!(
        "documents: {}  code points: {}  ({:.1} MB of text)",
        texts.len(),
        chars,
        texts.iter().map(|t| t.len()).sum::<usize>() as f64 / 1e6
    );

    for round in 0..3 {
        let start = Instant::now();
        let mut fonts = FontContract::default();
        for text in &texts {
            fonts.observe(text);
        }
        let elapsed = start.elapsed();
        println!(
            "  round {round}: {:>7.1} ms   ({:.1} ns per code point)   blocks: {}",
            elapsed.as_secs_f64() * 1e3,
            elapsed.as_nanos() as f64 / chars as f64,
            fonts.blocks.len()
        );
    }
}
