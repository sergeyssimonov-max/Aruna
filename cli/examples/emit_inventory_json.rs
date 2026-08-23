//! Emit `src/data/inventory.json` from the archive — the web catalog, built
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
//! The document itself is [`aruna::catalog`] — in the library, where it is
//! tested. What is left here is the archive in, the file out.

use aruna::archive::parse_zip;
use aruna::SOURCE_LABEL;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let (Some(zip), Some(out_path)) = (args.next(), args.next()) else {
        eprintln!("usage: emit_inventory_json <archive.zip> <out.json>");
        return ExitCode::FAILURE;
    };
    let zip = PathBuf::from(zip);
    let out_path = PathBuf::from(out_path);

    let records = match parse_zip(&zip, &aruna::job::Job::unattended()) {
        Ok(records) => records,
        Err(e) => {
            eprintln!("parse failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    let catalog = aruna::catalog::render(&records, SOURCE_LABEL);
    if let Err(e) = std::fs::write(&out_path, catalog.json.as_bytes()) {
        eprintln!("write {}: {e}", out_path.display());
        return ExitCode::FAILURE;
    }

    eprintln!(
        "wrote {} — {} manuscripts, {} pooled strings",
        out_path.display(),
        records.len(),
        catalog.pooled_strings
    );
    ExitCode::SUCCESS
}
