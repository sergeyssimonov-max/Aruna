//! Emit `public/data/inventory.json` from the archive — the web catalog, built
//! by the same parser the CLI uses.
//!
//! ```text
//! cargo run --release --example emit_inventory_json -- <archive.zip> <out.json>
//! ```
//!
//! Without this the site's catalog had no producer at all: `inventory.json` was
//! committed once and never regenerated, while the parser kept improving, so the
//! two descriptions of one corpus drifted apart with nothing to notice.
//!
//! The wire shape is the one `scripts/build-inventory-bin.mjs` reads:
//!
//! ```json
//! { "s": source, "m": count, "p": [pooled strings],
//!   "g": [[cth label, [[siglum, auth, year, lang, inv, corpus], …]], …],
//!   "v": 2 }
//! ```
//!
//! Metadata fields are indices into the pool `p`; grouping and order follow the
//! HTML exactly, so the site lists manuscripts in the order the CLI does.

use aruna::archive::parse_zip;
use aruna::parse::{group_label, ManuscriptRecord};
use aruna::SOURCE_LABEL;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

/// Interner: the pool holds each distinct string once, rows carry indices.
#[derive(Default)]
struct Pool {
    items: Vec<String>,
    index: HashMap<String, usize>,
}

impl Pool {
    fn intern(&mut self, s: &str) -> usize {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.items.len();
        self.items.push(s.to_string());
        self.index.insert(s.to_string(), i);
        i
    }
}

/// Minimal JSON string escaping — enough for this document, which holds only
/// catalogue text.
fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let (Some(zip), Some(out_path)) = (args.next(), args.next()) else {
        eprintln!("usage: emit_inventory_json <archive.zip> <out.json>");
        return ExitCode::FAILURE;
    };
    let zip = PathBuf::from(zip);
    let out_path = PathBuf::from(out_path);

    let records: Vec<ManuscriptRecord> = match parse_zip(&zip) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("parse failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut pool = Pool::default();
    let mut json = String::with_capacity(1 << 20);
    json.push('{');
    json.push_str("\"s\":");
    json_str(SOURCE_LABEL, &mut json);
    json.push_str(&format!(",\"m\":{}", records.len()));

    // Groups are consecutive runs of one CTH label, exactly as the HTML lays
    // them out — records arrive sorted from parse_zip.
    let mut groups = String::new();
    groups.push('[');
    let mut i = 0;
    let mut first_group = true;
    while i < records.len() {
        let label = group_label(&records[i]).to_string();
        let mut j = i;
        while j < records.len() && group_label(&records[j]) == label {
            j += 1;
        }

        if !first_group {
            groups.push(',');
        }
        first_group = false;
        groups.push('[');
        json_str(&label, &mut groups);
        groups.push_str(",[");
        for (n, rec) in records[i..j].iter().enumerate() {
            if n > 0 {
                groups.push(',');
            }
            let auth = pool.intern(&rec.authorship);
            let year = pool.intern(&rec.year);
            let lang = pool.intern(&rec.lang);
            let inv = pool.intern(&rec.inv);
            let corpus = pool.intern(&rec.corpus);
            groups.push('[');
            json_str(&rec.sigla, &mut groups);
            groups.push_str(&format!(",{auth},{year},{lang},{inv},{corpus}]"));
        }
        groups.push_str("]]");
        i = j;
    }
    groups.push(']');

    // The pool is only complete once every row has been interned, so it is
    // written after the groups are built.
    json.push_str(",\"p\":[");
    for (n, s) in pool.items.iter().enumerate() {
        if n > 0 {
            json.push(',');
        }
        json_str(s, &mut json);
    }
    json.push(']');
    json.push_str(",\"g\":");
    json.push_str(&groups);
    json.push_str(",\"v\":2}");

    if let Err(e) = std::fs::write(&out_path, json.as_bytes()) {
        eprintln!("write {}: {e}", out_path.display());
        return ExitCode::FAILURE;
    }
    eprintln!(
        "wrote {} — {} manuscripts, {} pooled strings",
        out_path.display(),
        records.len(),
        pool.items.len()
    );
    ExitCode::SUCCESS
}
